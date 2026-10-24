//! Existing noninteractive sudo only; an unproved helper leaves a durable no-repeat latch.
use super::*;
use std::{
    io::{self, Read},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt},
        process::{CommandExt, ExitStatusExt},
    },
    process::{Child, ChildStderr, ChildStdout, Stdio},
};

const RECORD: usize = 4096;
fn uncertain(message: impl std::fmt::Display) -> store::Error {
    failure(format!("{PRIVILEGED_UNCERTAIN} {message}"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    pid: i32,
    start: u64,
    euid: u32,
}
fn bounded_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(failure)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() > limit {
        return Err(failure("proc identity exceeds bound"));
    }
    Ok(bytes)
}
fn stat_start(bytes: &[u8], pid: i32) -> Result<u64> {
    let text = std::str::from_utf8(bytes).map_err(failure)?;
    if !text.ends_with('\n') || !text.starts_with(&format!("{pid} (")) {
        return Err(failure("invalid proc stat identity"));
    }
    let end = text
        .rfind(") ")
        .ok_or_else(|| failure("invalid proc stat command"))?;
    let start = text[end + 2..]
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| failure("missing proc starttime"))?
        .parse::<u64>()
        .map_err(failure)?;
    if start == 0 {
        return Err(failure("invalid proc starttime"));
    }
    Ok(start)
}
fn status_euid(bytes: &[u8]) -> Result<u32> {
    let text = std::str::from_utf8(bytes).map_err(failure)?;
    let mut uid = None;
    for line in text.lines().filter_map(|l| l.strip_prefix("Uid:")) {
        let fields = line
            .split_whitespace()
            .map(uid_token)
            .collect::<Result<Vec<_>>>()?;
        if fields.len() != 4 || uid.replace(fields[1]).is_some() {
            return Err(failure("invalid proc Uid record"));
        }
    }
    uid.ok_or_else(|| failure("missing proc effective UID"))
}
fn identity(pid: i32, start: Instant) -> Result<Identity> {
    deadline(start)?;
    let path = PathBuf::from(format!("/proc/{pid}"));
    let before = stat_start(&bounded_file(&path.join("stat"), RECORD)?, pid)?;
    deadline(start)?;
    let euid = status_euid(&bounded_file(&path.join("status"), 16 * 1024)?)?;
    deadline(start)?;
    let after = stat_start(&bounded_file(&path.join("stat"), RECORD)?, pid)?;
    deadline(start)?;
    if before != after {
        return Err(failure("proc PID reused during identity read"));
    }
    Ok(Identity {
        pid,
        start: before,
        euid,
    })
}
fn unchanged(before: Identity, after: Identity, uid: u32) -> Result<()> {
    if before != after || before.euid != uid {
        return Err(failure("protected cwd PID/starttime/effective UID changed"));
    }
    Ok(())
}
fn binary_mode(uid: u32, mode: u32, regular: bool, sudo: bool) -> Result<()> {
    if uid != 0 || !regular || mode & 0o022 != 0 || mode & 0o111 == 0 || sudo && mode & 0o4000 == 0
    {
        return Err(failure("protected cwd executable ownership/mode refused"));
    }
    Ok(())
}
fn binaries() -> Result<()> {
    for (path, sudo) in [("/usr/bin/sudo", true), ("/usr/bin/readlink", false)] {
        executable(Path::new(path), sudo)?;
    }
    Ok(())
}
fn executable(path: &Path, sudo: bool) -> Result<()> {
    let m = fs::metadata(path).map_err(failure)?;
    binary_mode(m.uid(), m.mode(), m.is_file(), sudo)
}
fn argv(pid: i32) -> Result<Vec<String>> {
    if pid <= 0 {
        return Err(failure("invalid protected cwd PID"));
    }
    Ok(vec![
        "/usr/bin/sudo".into(),
        "-n".into(),
        "--".into(),
        "/usr/bin/readlink".into(),
        "-z".into(),
        "--".into(),
        format!("/proc/{pid}/cwd"),
    ])
}
fn private(path: &Path, uid: u32, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(failure)?;
    let valid = if directory {
        m.is_dir() && m.mode() & 0o7777 == 0o700
    } else {
        m.is_file() && m.mode() & 0o7777 == 0o600
    };
    if m.uid() != uid || !valid {
        return Err(failure(
            "protected cwd artifact ownership/type/mode refused",
        ));
    }
    Ok(())
}
fn write_once(path: &Path, value: &Value) -> Result<()> {
    super::super::driver::write_once(path, &serde_json::to_vec(value).map_err(failure)?)
}
fn sync_dir(path: &Path) -> Result<()> {
    fs::File::open(path)
        .map_err(failure)?
        .sync_all()
        .map_err(failure)
}

pub(super) struct Reader {
    attempt: PathBuf,
    dir: PathBuf,
    uid: u32,
    start: Instant,
    reads: u32,
}
impl Reader {
    pub fn new(attempt: PathBuf, checkout: &Path, uid: u32, start: Instant) -> Result<Self> {
        let (resolved, _) = resolve_cwd(&attempt, false, start).map_err(failure)?;
        if !attempt.is_absolute() || resolved.starts_with(checkout) {
            return Err(failure("protected cwd artifacts must be outside checkout"));
        }
        let reader = Self {
            dir: attempt.join("protected-cwd"),
            attempt,
            uid,
            start,
            reads: 0,
        };
        reader.clear()?;
        Ok(reader)
    }
    fn clear(&self) -> Result<()> {
        let campaign = self
            .attempt
            .parent()
            .ok_or_else(|| failure("missing artifact campaign directory"))?;
        let base = campaign
            .parent()
            .ok_or_else(|| failure("missing private controller artifact root"))?;
        for path in [base, campaign, self.attempt.as_path(), self.dir.as_path()] {
            match fs::symlink_metadata(path) {
                Ok(_) => private(path, self.uid, true).map_err(|e| uncertain(e.message))?,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(uncertain(e)),
            }
        }
        let pending = self.dir.join("pending.json");
        match fs::symlink_metadata(&pending) {
            Ok(_) => {
                // Even malformed or partial intent is a no-repeat latch, never absence evidence.
                let detail = private(&pending, self.uid, false)
                    .err()
                    .map(|e| e.message)
                    .unwrap_or_default();
                Err(uncertain(format!(
                    "pending/unknown owned helper; reconcile {} {detail}",
                    pending.display()
                )))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(uncertain(e)),
        }
    }
    fn reserve(&mut self, before: Identity, args: &[String]) -> Result<PathBuf> {
        self.clear()?;
        deadline(self.start)?;
        if self.reads >= 16 {
            return Err(failure("protected cwd read cap16 reached"));
        }
        self.reads += 1;
        private_dir(&self.attempt)?;
        private(&self.attempt, self.uid, true)?;
        match fs::DirBuilder::new().mode(0o700).create(&self.dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(failure(e)),
        }
        private(&self.dir, self.uid, true)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(failure)?
            .as_nanos();
        let evidence = self.dir.join(format!(
            "read-{}-{nonce}-{}",
            std::process::id(),
            self.reads
        ));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&evidence)
            .map_err(failure)?;
        let intent = json!({"version":1,"evidence":evidence,"argv":args,"observer_pid":std::process::id(),"target_pid":before.pid,"target_start":before.start,"target_euid":before.euid});
        write_once(&self.dir.join("pending.json"), &intent).map_err(|e| uncertain(e.message))?;
        sync_dir(&self.dir).map_err(|e| uncertain(e.message))?;
        Ok(evidence)
    }
    fn finished(&self, evidence: &Path, value: Value) -> Result<()> {
        write_once(&evidence.join("finished.json"), &value).map_err(|e| uncertain(e.message))?;
        sync_dir(evidence).map_err(|e| uncertain(e.message))?;
        fs::remove_file(self.dir.join("pending.json")).map_err(uncertain)?;
        sync_dir(&self.dir).map_err(|e| uncertain(e.message))
    }
    pub fn read(&mut self, pid: i32) -> Result<PathBuf> {
        self.clear()?;
        if self.reads >= 16 {
            return Err(failure("protected cwd read cap16 reached"));
        }
        let before = identity(pid, self.start)?;
        unchanged(before, before, self.uid)?;
        binaries()?;
        let args = argv(pid)?;
        let evidence = self.reserve(before, &args)?;
        if let Err(e) = deadline(self.start) {
            self.finished(&evidence, json!({"spawned":false,"deadline":true}))?;
            return Err(e);
        }
        let mut command = Command::new(&args[0]);
        command
            .args(&args[1..])
            .env_clear()
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                self.finished(&evidence, json!({"spawned":false,"error":e.to_string()}))?;
                return Err(failure(e));
            }
        };
        let mut native = Native {
            child,
            stdout: None,
            stderr: None,
            out: Vec::new(),
            err: Vec::new(),
            status: None,
            signalled: false,
        };
        let helper = i32::try_from(native.child.id()).map_err(uncertain)?;
        write_once(
            &evidence.join("spawned.json"),
            &json!({"pid":helper,"argv":args}),
        )
        .map_err(|e| uncertain(e.message))?;
        let own = identity(helper, self.start).map_err(|e| {
            uncertain(format!(
                "helper {helper} identity unproved: {}; evidence={}",
                e.message,
                evidence.display()
            ))
        })?;
        write_once(
            &evidence.join("helper.json"),
            &json!({"pid":own.pid,"start":own.start,"euid":own.euid,"argv":args}),
        )
        .map_err(|e| uncertain(e.message))?;
        sync_dir(&evidence).map_err(|e| uncertain(e.message))?;
        native.attach().map_err(uncertain)?;
        let result = poll_helper(&mut native, self.start).map_err(|e| {
            uncertain(format!(
                "{}; helper={helper} start={} evidence={}",
                e.message,
                own.start,
                evidence.display()
            ))
        })?;
        self.finished(&evidence, json!({"pid":helper,"start":own.start,"argv":args,"reaped":true,"exit":result.code,"signal":result.signal,"stdout_retained_sha256":format!("{:x}",Sha256::digest(&result.stdout)),"stderr_retained_sha256":format!("{:x}",Sha256::digest(&result.stderr))}))?;
        let path = record(&result)?;
        unchanged(before, identity(pid, self.start)?, self.uid)?;
        deadline(self.start)?;
        Ok(path)
    }
}

#[derive(Clone, Debug)]
struct Completed {
    code: Option<i32>,
    signal: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
fn record(result: &Completed) -> Result<PathBuf> {
    let bytes = &result.stdout;
    if result.code != Some(0)
        || result.signal.is_some()
        || !result.stderr.is_empty()
        || bytes.len() < 2
        || bytes.len() > RECORD
        || !bytes.ends_with(&[0])
        || bytes[..bytes.len() - 1].contains(&0)
    {
        return Err(failure(
            "protected cwd command denied, failed or malformed NUL record",
        ));
    }
    let text = std::str::from_utf8(&bytes[..bytes.len() - 1]).map_err(failure)?;
    if !Path::new(text).is_absolute() {
        return Err(failure("protected cwd record is not absolute"));
    }
    Ok(PathBuf::from(text))
}
trait PollChild {
    fn poll(&mut self) -> io::Result<Option<Completed>>;
    fn stop(&mut self);
}
fn poll_helper(child: &mut impl PollChild, start: Instant) -> Result<Completed> {
    loop {
        if start.elapsed() >= BUDGET {
            child.stop();
            return Err(uncertain("helper deadline; termination unproved"));
        }
        match child.poll() {
            Ok(Some(done)) => return Ok(done),
            Ok(None) => {}
            Err(e) => {
                child.stop();
                return Err(uncertain(e));
            }
        }
        std::thread::sleep(Duration::from_millis(2).min(BUDGET.saturating_sub(start.elapsed())));
    }
}
struct Native {
    child: Child,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    out: Vec<u8>,
    err: Vec<u8>,
    status: Option<std::process::ExitStatus>,
    signalled: bool,
}
impl Native {
    fn attach(&mut self) -> io::Result<()> {
        self.stdout = self.child.stdout.take();
        self.stderr = self.child.stderr.take();
        super::super::process::nonblock(
            self.stdout
                .as_ref()
                .ok_or_else(|| io::Error::other("helper stdout missing"))?,
        )?;
        super::super::process::nonblock(
            self.stderr
                .as_ref()
                .ok_or_else(|| io::Error::other("helper stderr missing"))?,
        )
    }
}
fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>) -> io::Result<bool> {
    let mut buf = [0; 1024];
    for _ in 0..8 {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                bytes.extend_from_slice(&buf[..n.min((RECORD + 1).saturating_sub(bytes.len()))])
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(false)
}
impl PollChild for Native {
    fn poll(&mut self) -> io::Result<Option<Completed>> {
        let out = drain(
            self.stdout
                .as_mut()
                .ok_or_else(|| io::Error::other("helper stdout missing"))?,
            &mut self.out,
        )?;
        let err = drain(
            self.stderr
                .as_mut()
                .ok_or_else(|| io::Error::other("helper stderr missing"))?,
            &mut self.err,
        )?;
        if self.status.is_none() {
            self.status = self.child.try_wait()?;
        }
        Ok(
            if let Some(status) = self.status
                && out
                && err
            {
                Some(Completed {
                    code: status.code(),
                    signal: status.signal(),
                    stdout: std::mem::take(&mut self.out),
                    stderr: std::mem::take(&mut self.err),
                })
            } else {
                None
            },
        )
    }
    fn stop(&mut self) {
        if self.status.is_some() || self.signalled {
            return;
        }
        match self.child.try_wait() {
            Ok(Some(status)) => self.status = Some(status),
            Ok(None) => {
                self.signalled = true;
                if let Some(pid) = i32::try_from(self.child.id())
                    .ok()
                    .filter(|pid| *pid > 0)
                    .and_then(rustix::process::Pid::from_raw)
                {
                    let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
                }
                if let Ok(status) = self.child.try_wait() {
                    self.status = status;
                }
            }
            Err(_) => {}
        }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn fixture() -> (super::super::super::tests::Scratch, Reader, PathBuf) {
        let scratch = super::super::super::tests::Scratch::new();
        let attempt = scratch.0.join("controller/3/launch-11");
        let checkout = scratch.0.join("checkout");
        private_dir(&attempt).unwrap();
        fs::create_dir(&checkout).unwrap();
        let reader = Reader::new(
            attempt,
            &checkout,
            rustix::process::geteuid().as_raw(),
            Instant::now(),
        )
        .unwrap();
        (scratch, reader, checkout)
    }
    fn target(uid: u32) -> Identity {
        Identity {
            pid: 42,
            start: 12345,
            euid: uid,
        }
    }
    fn completion(bytes: &[u8]) -> Completed {
        Completed {
            code: Some(0),
            signal: None,
            stdout: bytes.to_vec(),
            stderr: Vec::new(),
        }
    }
    #[test]
    fn protected_record_requires_one_bounded_absolute_nul_record_and_clean_exit() {
        assert_eq!(
            record(&completion(b"/checkout/gone (deleted)\0")).unwrap(),
            PathBuf::from("/checkout/gone (deleted)")
        );
        for bytes in [
            b"".as_slice(),
            b"\0",
            b"/partial",
            b"relative\0",
            b"/one\0/two\0",
            b"/one\0\0",
            b"/bad\xff\0",
        ] {
            assert!(record(&completion(bytes)).is_err());
        }
        let maximum = format!("/{}\0", "x".repeat(RECORD - 2));
        assert!(record(&completion(maximum.as_bytes())).is_ok());
        let oversized = format!("/{}\0", "x".repeat(RECORD - 1));
        assert!(record(&completion(oversized.as_bytes())).is_err());
        for code in [Some(1), Some(2), None] {
            let mut result = completion(b"/checkout\0");
            result.code = code;
            assert!(record(&result).is_err());
        }
        let mut result = completion(b"/checkout\0");
        result.stderr = b"sudo denied\n".to_vec();
        assert!(record(&result).is_err());
        result.stderr.clear();
        result.signal = Some(15);
        assert!(record(&result).is_err());
    }
    #[test]
    fn proc_start_and_effective_uid_binding_refuses_reuse_and_credential_change() {
        let stat = format!("42 (a tricky ) name) S {} 12345\n", ["1"; 18].join(" "));
        assert_eq!(stat_start(stat.as_bytes(), 42).unwrap(), 12345);
        assert!(stat_start(stat.as_bytes(), 43).is_err());
        assert!(stat_start(b"42 (truncated) S 1\n", 42).is_err());
        assert_eq!(
            status_euid(b"Name:\tsynthetic\nUid:\t0\t501\t0\t0\n").unwrap(),
            501
        );
        for text in [
            "Uid: 501 501 501\n",
            "Uid: 501 nope 501 501\n",
            "Uid: 501 501 501 501\nUid: 501 501 501 501\n",
            "Name: synthetic\n",
        ] {
            assert!(status_euid(text.as_bytes()).is_err());
        }
        let before = target(501);
        assert!(unchanged(before, before, 501).is_ok());
        for after in [
            Identity {
                start: 12346,
                ..before
            },
            Identity { euid: 0, ..before },
            Identity { pid: 43, ..before },
        ] {
            assert!(unchanged(before, after, 501).is_err());
        }
        assert!(unchanged(target(0), target(0), 501).is_err());
    }
    #[test]
    fn privileged_argv_and_executable_validation_grant_no_new_permission() {
        assert_eq!(
            argv(42).unwrap(),
            [
                "/usr/bin/sudo",
                "-n",
                "--",
                "/usr/bin/readlink",
                "-z",
                "--",
                "/proc/42/cwd"
            ]
        );
        assert!(argv(0).is_err());
        assert!(argv(-1).is_err());
        let scratch = super::super::super::tests::Scratch::new();
        assert!(executable(&scratch.0.join("unavailable-sudo"), true).is_err());
        assert!(binary_mode(0, 0o4755, true, true).is_ok());
        assert!(binary_mode(0, 0o755, true, false).is_ok());
        for (uid, mode, regular, sudo) in [
            (501, 0o4755, true, true),
            (0, 0o4775, true, true),
            (0, 0o4757, true, true),
            (0, 0o755, true, true),
            (0, 0o4644, true, true),
            (0, 0o4755, false, true),
        ] {
            assert!(binary_mode(uid, mode, regular, sudo).is_err());
        }
        // Reference the Linux entry point for Mac test builds without executing a proc/sudo read.
        let _read: fn(&mut Reader, i32) -> Result<PathBuf> = Reader::read;
    }
    struct Controlled {
        polls: usize,
        stops: usize,
        result: Option<Completed>,
        error: bool,
    }
    impl PollChild for Controlled {
        fn poll(&mut self) -> io::Result<Option<Completed>> {
            self.polls += 1;
            if self.error {
                Err(io::Error::other("controlled read error"))
            } else {
                Ok(self.result.take())
            }
        }
        fn stop(&mut self) {
            self.stops += 1;
        }
    }
    #[test]
    fn helper_deadline_and_poll_errors_never_wait_for_privileged_cleanup() {
        let mut child = Controlled {
            polls: 0,
            stops: 0,
            result: None,
            error: false,
        };
        assert!(
            poll_helper(&mut child, Instant::now() - BUDGET)
                .unwrap_err()
                .message
                .starts_with(PRIVILEGED_UNCERTAIN)
        );
        assert_eq!((child.polls, child.stops), (0, 1));
        child.error = true;
        assert!(poll_helper(&mut child, Instant::now()).is_err());
        assert_eq!((child.polls, child.stops), (1, 2));
        child.error = false;
        child.result = Some(completion(b"/outside\0"));
        assert_eq!(
            poll_helper(&mut child, Instant::now()).unwrap().stdout,
            b"/outside\0".to_vec()
        );
        assert_eq!((child.polls, child.stops), (2, 2));
    }
    #[test]
    fn shared_unknown_intent_blocks_reader_reentry_without_a_second_helper() {
        let (_scratch, mut reader, checkout) = fixture();
        let args = argv(42).unwrap();
        let evidence = reader.reserve(target(reader.uid), &args).unwrap();
        write_once(
            &evidence.join("helper.json"),
            &json!({"pid":43,"start":999,"argv":args}),
        )
        .unwrap();
        private(&evidence.join("helper.json"), reader.uid, false).unwrap();
        let mut child = Controlled {
            polls: 0,
            stops: 0,
            result: None,
            error: false,
        };
        assert!(poll_helper(&mut child, Instant::now() - BUDGET).is_err());
        let error = Reader::new(
            reader.attempt.clone(),
            &checkout,
            reader.uid,
            Instant::now(),
        )
        .err()
        .unwrap();
        assert!(error.message.starts_with(PRIVILEGED_UNCERTAIN));
        assert!(reader.reserve(target(reader.uid), &args).is_err());
        assert_eq!(reader.reads, 1);
        assert!(reader.dir.join("pending.json").exists());
        assert!(!evidence.join("finished.json").exists());
    }
    #[test]
    fn known_completion_clears_latch_but_sixteen_read_quota_spans_attempts() {
        let (_scratch, mut reader, checkout) = fixture();
        for _ in 0..16 {
            let evidence = reader
                .reserve(target(reader.uid), &argv(42).unwrap())
                .unwrap();
            reader
                .finished(
                    &evidence,
                    json!({"reaped":true,"exit":1,"permission_denied":true}),
                )
                .unwrap();
        }
        assert!(!reader.dir.join("pending.json").exists());
        assert!(
            reader
                .reserve(target(reader.uid), &argv(42).unwrap())
                .unwrap_err()
                .message
                .contains("cap16")
        );
        assert_eq!(reader.reads, 16);
        assert!(
            Reader::new(
                reader.attempt.clone(),
                &checkout,
                reader.uid,
                Instant::now()
            )
            .is_ok()
        );
    }
    #[test]
    fn artifact_latch_refuses_partial_symlink_wrong_owner_mode_and_checkout_alias() {
        let (_scratch, mut reader, checkout) = fixture();
        assert!(Reader::new(checkout.clone(), &checkout, reader.uid, Instant::now()).is_err());
        let evidence = reader
            .reserve(target(reader.uid), &argv(42).unwrap())
            .unwrap();
        let pending = reader.dir.join("pending.json");
        fs::write(&pending, b"partial").unwrap();
        assert!(
            reader
                .clear()
                .unwrap_err()
                .message
                .starts_with(PRIVILEGED_UNCERTAIN)
        );
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(reader.clear().is_err());
        assert!(private(&pending, reader.uid.wrapping_add(1), false).is_err());
        fs::remove_file(&pending).unwrap();
        symlink(evidence.join("no-such-file"), &pending).unwrap();
        assert!(
            reader
                .clear()
                .unwrap_err()
                .message
                .starts_with(PRIVILEGED_UNCERTAIN)
        );
    }
}
