//! Cooperative evidence for one effective UID, never authority to kill an unexplained process.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

const MAX: usize = 4096;
const TEXT: usize = 1024;
const BUDGET: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    pid: i32,
    parent: i32,
    group: i32,
    tty: String,
    zombie: bool,
    command: String,
    session: i32,
}
#[derive(Default, PartialEq, Eq)]
pub(super) struct Trust {
    pub shells: BTreeMap<i32, WorkspaceShell>,
    pub root: BTreeMap<i32, String>,
    pub root_pane: String,
    pub root_group: i32,
    pub root_at: String,
}
#[derive(PartialEq, Eq)]
pub(super) struct WorkspaceShell {
    pub command: String,
    pub pane: String,
    pub group: i32,
}
impl Trust {
    pub fn same_basis(&self, other: &Self) -> bool {
        self.shells == other.shells
            && self.root == other.root
            && self.root_pane == other.root_pane
            && self.root_group == other.root_group
    }
}
fn pid(text: &str, zero: bool) -> Result<i32> {
    let n: i32 = text.parse().map_err(failure)?;
    if n < i32::from(!zero) {
        return Err(failure("invalid inventory PID"));
    }
    Ok(n)
}
fn uid_token(text: &str) -> Result<u32> {
    if text.len() > 11 {
        return Err(failure("oversized UID token"));
    }
    if text.starts_with('-') {
        text.parse::<i32>().map(i32::cast_unsigned).map_err(failure)
    } else {
        text.parse::<u32>().map_err(failure)
    }
}
fn parse_ps(bytes: &[u8], uid: u32) -> Result<Vec<Row>> {
    let text = std::str::from_utf8(bytes).map_err(failure)?;
    if text.is_empty() || !text.ends_with('\n') {
        return Err(failure("incomplete ps output"));
    }
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 8 || index == MAX {
            return Err(failure("invalid or bounded ps inventory"));
        }
        let effective = uid_token(fields[0])?;
        uid_token(fields[1])?;
        let id = pid(fields[2], true)?;
        if !seen.insert(id) {
            return Err(failure("duplicate ps PID"));
        }
        // ps user selectors differ across platforms; uid is explicitly the effective UID.
        if effective != uid {
            continue;
        }
        if id == 0 {
            return Err(failure("invalid own effective-UID PID"));
        }
        let command = fields[7..].join(" ");
        if command.len() > TEXT
            || command.chars().any(char::is_control)
            || fields[5].len() > 128
            || fields[6].len() > 16
            || !matches!(
                fields[6].chars().next(),
                Some('D' | 'I' | 'R' | 'S' | 'T' | 'U' | 'W' | 'X' | 'Z' | 't')
            )
        {
            return Err(failure("duplicate or invalid ps row"));
        }
        rows.push(Row {
            pid: id,
            parent: pid(fields[3], true)?,
            group: pid(fields[4], false)?,
            tty: fields[5].into(),
            zombie: fields[6].starts_with('Z'),
            command,
            session: 0,
        });
    }
    if rows.is_empty() {
        return Err(failure("empty process inventory"));
    }
    Ok(rows)
}
// lsof -F0 terminates fields with NUL and process/file records with newline.
#[cfg(any(target_os = "macos", test))]
fn parse_lsof(bytes: &[u8]) -> Result<BTreeMap<i32, PathBuf>> {
    if bytes.is_empty() {
        return Ok(BTreeMap::new());
    }
    if !bytes.ends_with(b"\0\n") {
        return Err(failure("incomplete lsof output"));
    }
    let mut result = BTreeMap::new();
    let mut current = None;
    let mut command = false;
    let mut cwd = false;
    for raw in bytes.split(|b| *b == 0) {
        if raw.is_empty() {
            return Err(failure("empty lsof field"));
        }
        let raw = raw.strip_prefix(b"\n").unwrap_or(raw);
        if raw.is_empty() {
            continue;
        }
        let value = std::str::from_utf8(&raw[1..]).map_err(failure)?;
        if value.is_empty() || value.len() > TEXT || value.chars().any(char::is_control) {
            return Err(failure("invalid lsof field"));
        }
        match raw[0] {
            b'p' => {
                if current.is_some() {
                    return Err(failure("partial lsof record"));
                }
                current = Some(pid(value, false)?);
                command = false;
                cwd = false;
                if result.len() == MAX {
                    return Err(failure("bounded lsof inventory"));
                }
            }
            b'c' if current.is_some() && !command && !cwd => command = true,
            b'f' if command && !cwd && value == "cwd" => cwd = true,
            b'n' if cwd => {
                let path = PathBuf::from(value);
                if !path.is_absolute()
                    || result
                        .insert(current.ok_or_else(|| failure("missing lsof PID"))?, path)
                        .is_some()
                {
                    return Err(failure("duplicate or relative lsof cwd"));
                }
                cwd = false;
                // A complete process has exactly one cwd record.
                command = false;
                current = None;
            }
            _ => return Err(failure("unexpected lsof field/order")),
        }
    }
    if current.is_some() {
        return Err(failure("partial or empty lsof inventory"));
    }
    Ok(result)
}
fn deadline(start: Instant) -> Result<()> {
    if start.elapsed() >= BUDGET {
        Err(failure("process inventory deadline"))
    } else {
        Ok(())
    }
}
fn alive(id: i32) -> Result<bool> {
    if id <= 0 {
        return Err(failure("invalid live-process PID"));
    }
    let pid =
        rustix::process::Pid::from_raw(id).ok_or_else(|| failure("invalid live-process PID"))?;
    // Signal-zero check: never sends a signal to an unexplained process.
    match rustix::process::test_kill_process(pid) {
        Ok(()) => Ok(true),
        Err(rustix::io::Errno::SRCH) => Ok(false),
        Err(e) => Err(failure(e)),
    }
}
fn probe(args: &[&str], start: Instant, probes: &mut BTreeSet<i32>) -> Result<Vec<u8>> {
    deadline(start)?;
    let mut cmd = Command::new(args[0]);
    cmd.args(&args[1..]).env("LC_ALL", "C").current_dir("/");
    let out = process::run(
        cmd,
        BUDGET.saturating_sub(start.elapsed()),
        &AtomicBool::new(false),
    )
    .map_err(failure)?;
    probes.insert(i32::try_from(out.pid).map_err(failure)?);
    probe_bytes(out, args[0] == "/usr/sbin/lsof")
}
fn probe_bytes(out: process::Output, cwd_probe: bool) -> Result<Vec<u8>> {
    if !(out.exit == Some(0) || cwd_probe && out.exit == Some(1))
        || out.timed_out
        || out.cancelled
        || out.signal.is_some()
        || out.stdout.digest().truncated
        || out.stderr.digest().truncated
        || !cwd_probe && !out.stderr.bytes.is_empty()
    {
        return Err(failure("process probe failed, partial, or exceeded bounds"));
    }
    Ok(out.stdout.bytes)
}
fn snapshot(uid: u32, start: Instant, probes: &mut BTreeSet<i32>) -> Result<Vec<Row>> {
    let bytes = probe(
        &[
            "/bin/ps",
            "-ww",
            "-A",
            "-o",
            "uid=,ruid=,pid=,ppid=,pgid=,tty=,stat=,comm=",
        ],
        start,
        probes,
    )?;
    let mut rows = Vec::new();
    for mut row in parse_ps(&bytes, uid)? {
        deadline(start)?;
        let pid = rustix::process::Pid::from_raw(row.pid)
            .ok_or_else(|| failure("invalid inventory PID"))?;
        // Reads the true POSIX session, including on macOS where ps sess is zero.
        row.session = match rustix::process::getsid(Some(pid)) {
            Ok(session) => session.as_raw_pid(),
            Err(e) => {
                if !alive(row.pid)? {
                    continue;
                }
                return Err(failure(format!("cannot read live process session: {e}")));
            }
        };
        rows.push(row);
    }
    Ok(rows)
}
fn same_command(a: &str, b: &str) -> bool {
    fn name(command: &str) -> Option<&str> {
        let name = Path::new(command).file_name()?.to_str()?;
        Some(match name.strip_prefix('-') {
            Some(shell @ ("sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "nu")) => shell,
            _ => name,
        })
    }
    name(a).zip(name(b)).is_some_and(|(a, b)| a == b)
}
fn tty(tty: &str) -> bool {
    !matches!(tty, "?" | "??" | "-")
}
fn invalid_cwd() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "invalid or oversized kernel cwd record",
    )
}
fn enoent(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(rustix::io::Errno::NOENT.raw_os_error())
}
fn resolve_cwd(raw: &Path, linux: bool, start: Instant) -> std::io::Result<(PathBuf, bool)> {
    deadline(start).map_err(|e| std::io::Error::new(std::io::ErrorKind::TimedOut, e.message))?;
    let text = raw
        .to_str()
        .filter(|s| !s.is_empty() && s.len() <= TEXT && !s.chars().any(char::is_control))
        .ok_or_else(invalid_cwd)?;
    if !raw.is_absolute() {
        return Err(invalid_cwd());
    }
    match fs::canonicalize(raw) {
        Ok(path) => return Ok((path, false)),
        Err(e) if enoent(&e) => {}
        Err(e) => return Err(e),
    }
    let mut ancestor = if linux {
        text.strip_suffix(" (deleted)").unwrap_or(text)
    } else {
        text
    }
    .to_owned();
    let mut suffix = Vec::new();
    loop {
        deadline(start)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::TimedOut, e.message))?;
        match fs::canonicalize(&ancestor) {
            Ok(mut path) => {
                for component in suffix.iter().rev() {
                    path.push(component);
                }
                if path.as_os_str().len() > TEXT {
                    return Err(invalid_cwd());
                }
                return Ok((path, true));
            }
            Err(e) if enoent(&e) => {
                let split = ancestor.rfind('/').ok_or_else(invalid_cwd)?;
                let component = &ancestor[split + 1..];
                if matches!(component, "" | "." | "..") {
                    return Err(invalid_cwd());
                }
                suffix.push(component.to_owned());
                ancestor.truncate(split);
                if ancestor.is_empty() {
                    ancestor.push('/');
                }
            }
            Err(e) => return Err(e),
        }
    }
}
#[allow(clippy::too_many_arguments)] // Exact OS observations and trust inputs are independently fixture-controlled.
fn classified_evidence(
    rows: &[Row],
    cwds: &BTreeMap<i32, PathBuf>,
    checkout: &Path,
    shell: i32,
    observer: i32,
    uid: u32,
    probes: &BTreeSet<i32>,
    trust: &Trust,
    deleted: &BTreeSet<i32>,
) -> Result<Value> {
    if rows.is_empty() || rows.len() > MAX {
        return Err(failure("invalid inventory size"));
    }
    let target = rows
        .iter()
        .find(|r| r.pid == shell)
        .ok_or_else(|| failure("bound shell disappeared"))?;
    if !rows.iter().any(|r| r.pid == observer) {
        return Err(failure("observer absent from inventory"));
    }
    for (id, name) in trust
        .root
        .iter()
        .map(|(id, name)| (id, name.as_str()))
        .chain(trust.shells.iter().map(|(id, s)| (id, s.command.as_str())))
    {
        if !rows
            .iter()
            .any(|r| r.pid == *id && !r.zombie && same_command(name, &r.command))
        {
            return Err(failure(
                "attested root/shell identity missing from inventory",
            ));
        }
    }
    let mut descendants = BTreeSet::from([shell]);
    loop {
        let old = descendants.len();
        for r in rows {
            if descendants.contains(&r.parent) {
                descendants.insert(r.pid);
            }
        }
        if descendants.len() == old {
            break;
        }
    }
    let target_pane = &trust
        .shells
        .get(&shell)
        .ok_or_else(|| failure("target shell has no observed pane"))?
        .pane;
    let mut exempt = BTreeMap::new();
    if !deleted.contains(&observer) {
        exempt.insert(observer,json!({"pid":observer,"reason":"observer","observer_pid":observer,"parent_chain":[observer]}));
    }
    for r in rows {
        if deleted.contains(&r.pid) {
            continue;
        }
        if probes.contains(&r.pid) {
            let mut chain = vec![r.pid];
            while *chain.last().unwrap() != observer {
                let parent = rows
                    .iter()
                    .find(|row| row.pid == *chain.last().unwrap())
                    .ok_or_else(|| failure("probe PPID chain missing"))?
                    .parent;
                if parent <= 0 || chain.contains(&parent) {
                    return Err(failure("invalid probe PPID chain"));
                }
                chain.push(parent);
            }
            exempt.insert(r.pid,json!({"pid":r.pid,"reason":"observer","observer_pid":observer,"parent_chain":chain}));
        }
        if trust
            .root
            .get(&r.pid)
            .is_some_and(|name| same_command(name, &r.command))
        {
            if trust.root_pane == *target_pane
                || trust.root_pane.is_empty()
                || trust.root_group != r.group
            {
                return Err(failure("root foreground exemption pane/group mismatch"));
            }
            exempt.insert(r.pid,json!({"pid":r.pid,"reason":"campaign-root","pane":trust.root_pane,"foreground_pgid":trust.root_group,"foreground_pids":trust.root.keys().collect::<Vec<_>>(),"observed_at":trust.root_at}));
        }
        if trust
            .shells
            .get(&r.pid)
            .is_some_and(|s| same_command(&s.command, &r.command) && s.group == r.group)
            && r.group == r.pid
            && tty(&r.tty)
            && !rows
                .iter()
                .any(|other| other.pid != r.pid && !other.zombie && other.session == r.session)
        {
            let s = &trust.shells[&r.pid];
            exempt.insert(r.pid,json!({"pid":r.pid,"reason":"workspace-shell","pane":s.pane,"session_id":r.session,"foreground_pgid":s.group,"session_pids":[r.pid]}));
        }
    }
    let mut session = Vec::new();
    let mut checkout_remaining = Vec::new();
    for r in rows {
        if r.zombie || probes.contains(&r.pid) && !deleted.contains(&r.pid) {
            continue;
        }
        if exempt.contains_key(&r.pid) {
            continue;
        }
        let cwd = cwds
            .get(&r.pid)
            .ok_or_else(|| failure("live process cwd missing without validated exemption"))?;
        if descendants.contains(&r.pid)
            || r.session == target.session
            || tty(&target.tty) && r.tty == target.tty
        {
            session.push(json!({"pid":r.pid,"command":r.command}));
        }
        if cwd.starts_with(checkout) {
            checkout_remaining.push(json!({"pid":r.pid,"cwd":cwd}));
        }
    }
    let deleted_cwd = rows
        .iter()
        .filter(|r| !r.zombie && deleted.contains(&r.pid))
        .count();
    if session.len() > 1024 || checkout_remaining.len() > 1024 || exempt.len() > 1024 {
        return Err(failure("process evidence array bound exceeded"));
    }
    Ok(
        json!({"method":"local-user-session-cwd-v1","uid":uid,"shell_pid":shell,"session_id":target.session,
        "scanned":rows.len(),"deleted_cwd":deleted_cwd,"session_remaining":session,"checkout_remaining":checkout_remaining,
        "exempt":exempt.into_values().collect::<Vec<_>>()}),
    )
}
pub(super) fn scan(checkout: &Path, shell: i32, trust: &Trust) -> Result<Value> {
    let start = Instant::now();
    let uid = rustix::process::geteuid().as_raw();
    let observer = i32::try_from(std::process::id()).map_err(failure)?;
    let checkout = fs::canonicalize(checkout).map_err(failure)?;
    retry_pair(start, |start| {
        scan_pair(&checkout, shell, trust, uid, observer, start)
    })
}
fn retry_pair(
    start: Instant,
    mut pair: impl FnMut(Instant) -> Result<Option<Value>>,
) -> Result<Value> {
    loop {
        deadline(start)?;
        if let Some(scan) = pair(start)? {
            deadline(start)?;
            return Ok(scan);
        }
    }
}
fn stable_rows(first: &[Row], last: &[Row], probes: &BTreeSet<i32>) -> bool {
    last.iter()
        .all(|r| probes.contains(&r.pid) || first.contains(r))
}
fn scan_pair(
    checkout: &Path,
    shell: i32,
    trust: &Trust,
    uid: u32,
    observer: i32,
    start: Instant,
) -> Result<Option<Value>> {
    let mut probes = BTreeSet::new();
    let first = snapshot(uid, start, &mut probes)?;
    #[cfg(target_os = "macos")]
    let lsof = parse_lsof(&probe(
        &[
            "/usr/sbin/lsof",
            "-a",
            "-u",
            &uid.to_string(),
            "-d",
            "cwd",
            "-F0pcfn",
        ],
        start,
        &mut probes,
    )?)?;
    let mut cwds = BTreeMap::new();
    let mut deleted = BTreeSet::new();
    for r in &first {
        deadline(start)?;
        if r.zombie || probes.contains(&r.pid) {
            continue;
        }
        #[cfg(target_os = "linux")]
        let path = fs::read_link(format!("/proc/{}/cwd", r.pid)).map(Some);
        #[cfg(target_os = "macos")]
        let path: std::io::Result<Option<PathBuf>> = Ok(lsof.get(&r.pid).cloned());
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let path: std::io::Result<Option<PathBuf>> =
            Err(std::io::Error::other("unsupported process inventory OS"));
        match path {
            Ok(Some(raw)) => match resolve_cwd(&raw, cfg!(target_os = "linux"), start) {
                Ok((path, was_deleted)) => {
                    if path.as_os_str().len() > TEXT {
                        return Err(failure("canonical cwd exceeds bound"));
                    }
                    cwds.insert(r.pid, path);
                    if was_deleted {
                        deleted.insert(r.pid);
                    }
                }
                Err(_) if !alive(r.pid)? => {}
                Err(e) => return Err(failure(format!("cwd classification incomplete: {e}"))),
            },
            Ok(None) => {} // No lsof record: only an existing exact typed exemption can cover it.
            Err(_) if !alive(r.pid)? => {}
            Err(e) => return Err(failure(format!("cwd record unavailable: {e}"))),
        }
    }
    let last = snapshot(uid, start, &mut probes)?;
    for old in &first {
        deadline(start)?;
        if !probes.contains(&old.pid) && !last.iter().any(|r| r.pid == old.pid) && alive(old.pid)? {
            return Ok(None);
        }
    }
    if !stable_rows(&first, &last, &probes) {
        return Ok(None);
    }
    deadline(start)?;
    let scan = classified_evidence(
        &last, &cwds, checkout, shell, observer, uid, &probes, trust, &deleted,
    )?;
    deadline(start)?;
    Ok(Some(scan))
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn evidence(
    rows: &[Row],
    cwds: &BTreeMap<i32, PathBuf>,
    checkout: &Path,
    shell: i32,
    observer: i32,
    uid: u32,
    probes: &BTreeSet<i32>,
    trust: &Trust,
) -> Result<Value> {
    classified_evidence(
        rows,
        cwds,
        checkout,
        shell,
        observer,
        uid,
        probes,
        trust,
        &BTreeSet::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell(command: &str, pane: &str, group: i32) -> WorkspaceShell {
        WorkspaceShell {
            command: command.into(),
            pane: pane.into(),
            group,
        }
    }
    fn row(id: i32, parent: i32, session: i32, tty: &str, command: &str) -> Row {
        Row {
            pid: id,
            parent,
            group: id,
            session,
            tty: tty.into(),
            command: command.into(),
            zombie: false,
        }
    }
    #[test]
    fn deleted_kernel_cwds_classify_inside_outside_and_resolved_alias_ancestor() {
        use std::os::unix::fs::symlink;
        let scratch = super::super::tests::Scratch::new();
        fs::create_dir(scratch.0.join("checkout")).unwrap();
        fs::create_dir(scratch.0.join("outside")).unwrap();
        symlink(scratch.0.join("checkout"), scratch.0.join("alias")).unwrap();
        let checkout = fs::canonicalize(scratch.0.join("checkout")).unwrap();
        let rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            row(12, 1, 12, "?", "writer"),
        ];
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            ..Default::default()
        };
        for linux in [false, true] {
            for (relative, inside) in [
                ("checkout/gone/child", true),
                ("outside/gone/child", false),
                ("alias/gone/child", true),
            ] {
                let path = scratch.0.join(relative);
                let raw = if linux {
                    PathBuf::from(format!("{} (deleted)", path.display()))
                } else {
                    path
                };
                let (cwd, deleted) = resolve_cwd(&raw, linux, Instant::now()).unwrap();
                assert!(deleted);
                assert_eq!(cwd.starts_with(&checkout), inside);
                let scan = classified_evidence(
                    &rows,
                    &BTreeMap::from([(12, cwd)]),
                    &checkout,
                    10,
                    20,
                    501,
                    &BTreeSet::new(),
                    &trust,
                    &BTreeSet::from([12]),
                )
                .unwrap();
                assert_eq!(scan["deleted_cwd"], 1);
                assert_eq!(
                    scan["checkout_remaining"].as_array().unwrap().len(),
                    usize::from(inside)
                );
                assert_eq!(scan["session_remaining"], json!([]));
                assert!(
                    !scan["exempt"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["pid"] == 12)
                );
                let _: api::ProcessScan = serde_json::from_value(scan).unwrap();
            }
        }
        // The checkout root itself must resolve, even if a kernel cwd could be classified.
        assert!(scan(&scratch.0.join("missing-checkout"), 10, &trust).is_err());
        // No kernel cwd record cannot be reinterpreted as a deleted directory.
        assert!(
            evidence(
                &rows,
                &BTreeMap::new(),
                &checkout,
                10,
                20,
                501,
                &BTreeSet::new(),
                &trust
            )
            .is_err()
        );
    }
    #[test]
    fn deleted_cwd_suffix_validation_and_non_enoent_fail_closed() {
        use std::os::unix::fs::symlink;
        let scratch = super::super::tests::Scratch::new();
        for suffix in ["gone/../child", "gone/./child", "gone//child", "gone/"] {
            assert!(
                resolve_cwd(&scratch.0.join(suffix), false, Instant::now()).is_err(),
                "{suffix}"
            );
        }
        fs::write(scratch.0.join("file"), "not a directory").unwrap();
        let err = resolve_cwd(&scratch.0.join("file/child"), false, Instant::now()).unwrap_err();
        assert_eq!(
            err.raw_os_error(),
            Some(rustix::io::Errno::NOTDIR.raw_os_error())
        );
        symlink("loop", scratch.0.join("loop")).unwrap();
        let err = resolve_cwd(&scratch.0.join("loop/child"), false, Instant::now()).unwrap_err();
        assert_eq!(
            err.raw_os_error(),
            Some(rustix::io::Errno::LOOP.raw_os_error())
        );
        for errno in [
            rustix::io::Errno::ACCESS,
            rustix::io::Errno::NOTDIR,
            rustix::io::Errno::LOOP,
        ] {
            assert!(!enoent(&std::io::Error::from_raw_os_error(
                errno.raw_os_error()
            )));
        }
        let marker = scratch.0.join("gone (deleted) (deleted)");
        let (cwd, deleted) = resolve_cwd(&marker, true, Instant::now()).unwrap();
        assert!(deleted);
        assert_eq!(cwd, scratch.0.join("gone (deleted)")); // Strip exactly one marker.
        let literal = scratch.0.join("live (deleted)");
        fs::create_dir(&literal).unwrap();
        assert_eq!(
            resolve_cwd(&literal, true, Instant::now()).unwrap(),
            (literal, false)
        );
        for raw in [
            "relative/gone".to_owned(),
            "/bad\npath".into(),
            format!("/{}", "x".repeat(TEXT)),
        ] {
            assert!(resolve_cwd(Path::new(&raw), false, Instant::now()).is_err());
        }
        assert!(resolve_cwd(&scratch.0.join("gone"), false, Instant::now() - BUDGET).is_err());
    }
    #[test]
    fn deleted_cwds_never_receive_shell_root_observer_or_probe_exemptions() {
        let mut rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            row(21, 20, 20, "?", "ps"),
            row(30, 1, 30, "ttys2", "taskr"),
        ];
        let mut zombie = row(40, 1, 40, "?", "writer");
        zombie.zombie = true;
        rows.push(zombie);
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            root: BTreeMap::from([(30, "taskr".into())]),
            root_pane: "w2:p1".into(),
            root_group: 30,
            root_at: "2026-01-01T00:00:00Z".into(),
        };
        let cwds = rows
            .iter()
            .map(|r| (r.pid, PathBuf::from("/outside/gone")))
            .collect();
        let deleted = rows.iter().map(|r| r.pid).collect();
        let scan = classified_evidence(
            &rows,
            &cwds,
            Path::new("/checkout"),
            10,
            20,
            501,
            &BTreeSet::from([21]),
            &trust,
            &deleted,
        )
        .unwrap();
        assert_eq!(scan["deleted_cwd"], 4);
        assert_eq!(scan["exempt"], json!([]));
        assert_eq!(scan["checkout_remaining"], json!([]));
        assert_eq!(scan["session_remaining"][0]["pid"], 10);
    }
    #[test]
    fn signed_uid_tokens_normalize_before_filtering_without_exempting_names() {
        let bytes=b"-2 -2 3305 1 3305 ?? Ss /usr/libexec/dhcp6d\n4294967294 501 3306 1 3306 ttys1 S zsh\n-2 4294967294 3307 1 3307 ? S taskr\n501 -2 10 1 10 ttys2 S zsh\n501 501 20 1 20 ? S taskr\n";
        assert_eq!(
            parse_ps(bytes, 501)
                .unwrap()
                .iter()
                .map(|r| r.pid)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
        let high = 4294967294;
        let mut rows = parse_ps(bytes, high).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.pid).collect::<Vec<_>>(),
            vec![3305, 3306, 3307]
        );
        for r in &mut rows {
            r.session = r.pid;
        }
        let trust = Trust {
            shells: BTreeMap::from([(3306, shell("zsh", "w1:p1", 3306))]),
            ..Default::default()
        };
        assert!(
            evidence(
                &rows,
                &BTreeMap::new(),
                Path::new("/checkout"),
                3306,
                3307,
                high,
                &BTreeSet::new(),
                &trust
            )
            .is_err()
        );
        let scan = evidence(
            &rows,
            &BTreeMap::from([(3305, PathBuf::from("/checkout/sub"))]),
            Path::new("/checkout"),
            3306,
            3307,
            high,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"][0]["pid"], 3305);
        for (text, expected) in [
            ("-2", 4294967294),
            ("-1", u32::MAX),
            ("4294967295", u32::MAX),
            ("-2147483648", 2147483648),
            ("0", 0),
        ] {
            assert_eq!(uid_token(text).unwrap(), expected);
        }
        for text in [
            "4294967296",
            "-2147483649",
            "--2",
            "nobody",
            "",
            "000000000000",
        ] {
            assert!(uid_token(text).is_err());
        }
    }
    #[test]
    fn effective_uid_filter_excludes_foreign_setuid_but_requires_own_cwd() {
        let mut rows=parse_ps(b"0 0 0 0 0 ?? R kernel_task\n0 501 74509 1 74509 ttys1 S /usr/bin/login\n501 501 10 1 10 ttys2 S zsh\n501 501 20 1 20 ? S taskr\n501 0 12 1 12 ? S /usr/bin/login\n",501).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.pid).collect::<Vec<_>>(),
            vec![10, 20, 12]
        );
        for r in &mut rows {
            r.session = r.pid;
        }
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            ..Default::default()
        };
        assert!(
            evidence(
                &rows,
                &BTreeMap::new(),
                Path::new("/checkout"),
                10,
                20,
                501,
                &BTreeSet::new(),
                &trust
            )
            .is_err()
        );
        let scan = evidence(
            &rows,
            &BTreeMap::from([(12, PathBuf::from("/checkout/sub"))]),
            Path::new("/checkout"),
            10,
            20,
            501,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"][0]["pid"], 12);
        assert_eq!(scan["uid"], 501);
        rows.retain(|r| r.pid != 12);
        let scan = evidence(
            &rows,
            &BTreeMap::new(),
            Path::new("/checkout"),
            10,
            20,
            501,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"], json!([]));
        let foreign = (1..=MAX + 1)
            .map(|id| format!("0 501 {id} 1 {id} ? S login\n"))
            .collect::<String>();
        assert!(
            parse_ps(foreign.as_bytes(), 501)
                .unwrap_err()
                .message
                .contains("bounded")
        );
    }
    #[test]
    fn snapshot_pair_churn_retries_with_one_budget_and_preserves_writer_coverage() {
        let first = vec![row(10, 1, 10, "ttys1", "zsh"), row(20, 1, 20, "?", "taskr")];
        let mut last = first.clone();
        last.push(row(12, 1, 12, "?", "writer"));
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            ..Default::default()
        };
        let start = Instant::now();
        let mut calls = 0;
        let scan = retry_pair(start, |bound| {
            assert_eq!(bound, start);
            calls += 1;
            let before = if calls == 1 { &first } else { &last };
            if !stable_rows(before, &last, &BTreeSet::new()) {
                return Ok(None);
            }
            evidence(
                &last,
                &BTreeMap::from([(12, PathBuf::from("/checkout/sub"))]),
                Path::new("/checkout"),
                10,
                20,
                501,
                &BTreeSet::new(),
                &trust,
            )
            .map(Some)
        })
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(scan["checkout_remaining"][0]["pid"], 12);
        let mut calls = 0;
        assert!(
            retry_pair(Instant::now() - BUDGET, |_| {
                calls += 1;
                Ok(Some(json!({})))
            })
            .is_err()
        );
        assert_eq!(calls, 0);
        assert!(
            retry_pair(Instant::now(), |_| Err(failure(
                "synthetic permission failure"
            )))
            .is_err()
        );
    }
    #[test]
    fn login_shell_normalization_preserves_exact_inventory_exemptions() {
        let mut rows = parse_ps(
            b"501 501 17576 1 17576 ttys023 Ss+ -zsh\n501 501 600 1 600 ? S taskr\n",
            501,
        )
        .unwrap();
        for r in &mut rows {
            r.session = r.pid;
        }
        let trust = Trust {
            shells: BTreeMap::from([(17576, shell("zsh", "w1:p1", 17576))]),
            ..Default::default()
        };
        let scan = evidence(
            &rows,
            &BTreeMap::new(),
            Path::new("/checkout"),
            17576,
            600,
            501,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        let exempt = scan["exempt"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["pid"] == 17576)
            .unwrap();
        assert_eq!(
            exempt,
            &json!({"pid":17576,"reason":"workspace-shell","pane":"w1:p1","session_id":17576,"foreground_pgid":17576,"session_pids":[17576]})
        );
        rows.push(row(17577, 1, 17577, "?", "-zsh"));
        let scan = evidence(
            &rows,
            &BTreeMap::from([(17577, PathBuf::from("/checkout/sub"))]),
            Path::new("/checkout"),
            17576,
            600,
            501,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"][0]["pid"], 17577);
        assert!(
            !scan["exempt"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["pid"] == 17577)
        );
        rows[0].group = 17578;
        assert!(
            evidence(
                &rows,
                &BTreeMap::new(),
                Path::new("/checkout"),
                17576,
                600,
                501,
                &BTreeSet::new(),
                &trust
            )
            .is_err()
        );
    }
    #[test]
    fn command_normalization_only_strips_one_supported_login_shell_dash() {
        for shell in ["sh", "bash", "zsh", "fish", "dash", "ksh", "nu"] {
            assert!(same_command(&format!("-{shell}"), &format!("/bin/{shell}")));
            assert!(same_command(&format!("/bin/{shell}"), &format!("-{shell}")));
        }
        for (a, b) in [
            ("-taskr", "taskr"),
            ("--zsh", "zsh"),
            ("-zsh-helper", "zsh-helper"),
            ("-zsh", "bash"),
            ("/", "/"),
        ] {
            assert!(!same_command(a, b));
        }
        assert!(same_command("/synthetic/bin/codex", "codex"));
    }
    fn fixture(extra: Vec<Row>, cwd: &str, probes: BTreeSet<i32>) -> Result<Value> {
        let mut rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            row(30, 1, 30, "ttys3", "claude"),
            row(40, 1, 40, "ttys4", "bash"),
        ];
        rows.extend(extra);
        let cwds = rows
            .iter()
            .map(|r| {
                (
                    r.pid,
                    PathBuf::from(if r.pid > 40 || r.pid == 11 || r.pid == 12 {
                        cwd
                    } else {
                        "/checkout"
                    }),
                )
            })
            .collect();
        let trust = Trust {
            shells: BTreeMap::from([
                (10, shell("zsh", "w1:p1", 10)),
                (40, shell("bash", "w1:p4", 40)),
            ]),
            root: BTreeMap::from([(30, "claude".into())]),
            root_pane: "w2:p1".into(),
            root_group: 30,
            root_at: "2026-01-01T00:00:00Z".into(),
        };
        evidence(
            &rows,
            &cwds,
            Path::new("/checkout"),
            10,
            20,
            501,
            &probes,
            &trust,
        )
    }
    #[test]
    fn parsers_reject_partial_duplicate_invalid_and_bounded_inventories() {
        assert_eq!(
            parse_ps(b"501 501 10 1 10 ttys1 S /bin/zsh\n", 501).unwrap()[0].pid,
            10
        );
        for bad in [
            b"".as_slice(),
            b"501 501 10 1 10 ttys1 S zsh",
            b"501 501 10 bad 10 ttys1 S zsh\n",
            b"501 501 0 1 1 ? S zsh\n",
            b"501 501 10 1 10 ? S\n",
            b"501 501 10 1 10 ? S zsh\n501 501 10 1 10 ? S zsh\n",
        ] {
            assert!(parse_ps(bad, 501).is_err());
        }
        let large = (1..=MAX + 1)
            .map(|id| format!("501 501 {id} 1 {id} ? S taskr\n"))
            .collect::<String>();
        assert!(parse_ps(large.as_bytes(), 501).is_err());
        let valid = b"p10\0czsh\0\nfcwd\0n/checkout with spaces\0\np20\0ctaskr\0\nfcwd\0n/\0\n";
        assert_eq!(
            parse_lsof(valid).unwrap()[&10],
            Path::new("/checkout with spaces")
        );
        for bad in [
            b"p10\0czsh\0\nfcwd\0n/a".as_slice(),
            b"p10\0czsh\0\nfcwd\0nrelative\0\n".as_slice(),
            b"p10\0czsh\0\nfcwd\0n/a\0\np10\0czsh\0\nfcwd\0n/b\0\n".as_slice(),
            b"p10\0czsh\0\np20\0ctaskr\0\nfcwd\0n/a\0\n".as_slice(),
            b"p10\0czsh\0\nfcwd\0n/a\0\nxunexpected\0\n".as_slice(),
        ] {
            assert!(parse_lsof(bad).is_err());
        }
        let large = (1..=MAX + 1)
            .map(|id| format!("p{id}\0ctaskr\0\nfcwd\0n/\0\n"))
            .collect::<String>();
        assert!(parse_lsof(large.as_bytes()).is_err());
        assert!(parse_lsof(b"").unwrap().is_empty());
    }
    #[test]
    fn session_background_descendant_and_shared_tty_block() {
        for child in [
            row(11, 10, 10, "ttys1", "background"),
            row(11, 10, 11, "?", "detached-descendant"),
            row(11, 1, 11, "ttys1", "shared-tty"),
        ] {
            let scan = fixture(vec![child], "/outside", BTreeSet::new()).unwrap();
            assert!(
                scan["session_remaining"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["pid"] == 11)
            );
        }
    }
    #[test]
    fn detached_checkout_child_blocks_but_sibling_path_does_not() {
        let scan = fixture(
            vec![row(12, 1, 12, "?", "taskr")],
            "/checkout/sub",
            BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"][0]["pid"], 12);
        let scan = fixture(
            vec![row(12, 1, 12, "?", "taskr")],
            "/checkout-sibling",
            BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"], json!([]));
        assert_eq!(scan["session_remaining"], json!([]));
    }
    #[test]
    fn exemptions_are_exact_and_busy_workspace_shell_is_not_exempt() {
        let scan = fixture(vec![], "/outside", BTreeSet::new()).unwrap();
        let _: api::ProcessScan = serde_json::from_value(scan.clone()).unwrap();
        assert_eq!(
            scan["exempt"],
            json!([{"pid":10,"reason":"workspace-shell","pane":"w1:p1","session_id":10,"foreground_pgid":10,"session_pids":[10]},
                {"pid":20,"reason":"observer","observer_pid":20,"parent_chain":[20]},
                {"pid":30,"reason":"campaign-root","pane":"w2:p1","foreground_pgid":30,"foreground_pids":[30],"observed_at":"2026-01-01T00:00:00Z"},
                {"pid":40,"reason":"workspace-shell","pane":"w1:p4","session_id":40,"foreground_pgid":40,"session_pids":[40]}])
        );
        let scan = fixture(
            vec![
                row(51, 20, 51, "?", "taskr"),
                row(52, 30, 52, "?", "claude"),
            ],
            "/checkout",
            BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"].as_array().unwrap().len(), 2);
        let scan = fixture(
            vec![row(51, 20, 51, "?", "ps")],
            "/checkout",
            BTreeSet::from([51]),
        )
        .unwrap();
        assert!(
            scan["exempt"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["pid"] == 51 && p["reason"] == "observer")
        );
        assert_eq!(scan["checkout_remaining"], json!([]));
        let scan = fixture(
            vec![row(51, 40, 40, "ttys4", "background")],
            "/outside",
            BTreeSet::new(),
        )
        .unwrap();
        assert!(
            !scan["exempt"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["pid"] == 40)
        );
        assert_eq!(scan["checkout_remaining"][0]["pid"], 40);
    }
    #[test]
    fn incomplete_cwd_or_attested_identity_fails_closed() {
        let rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            row(51, 1, 51, "?", "unexplained"),
        ];
        for name in ["zsh", "replacement"] {
            let trust = Trust {
                shells: BTreeMap::from([(10, shell(name, "w1:p1", 10))]),
                ..Default::default()
            };
            assert!(
                evidence(
                    &rows,
                    &BTreeMap::new(),
                    Path::new("/checkout"),
                    10,
                    20,
                    501,
                    &BTreeSet::new(),
                    &trust
                )
                .is_err()
            );
        }
    }
    #[test]
    fn canonical_configured_and_observed_symlink_aliases_match() {
        use std::os::unix::fs::symlink;
        #[cfg(target_os = "macos")]
        assert_eq!(
            fs::canonicalize("/tmp").unwrap(),
            fs::canonicalize("/private/tmp").unwrap()
        );
        let scratch = super::super::tests::Scratch::new();
        fs::create_dir_all(scratch.0.join("real/sub")).unwrap();
        symlink(scratch.0.join("real"), scratch.0.join("alias")).unwrap();
        let checkout = fs::canonicalize(scratch.0.join("alias")).unwrap();
        let cwd = fs::canonicalize(scratch.0.join("alias/sub")).unwrap();
        assert_eq!(cwd, scratch.0.join("real/sub"));
        let rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            row(12, 1, 12, "?", "writer"),
        ];
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            ..Default::default()
        };
        let scan = evidence(
            &rows,
            &BTreeMap::from([(12, cwd)]),
            &checkout,
            10,
            20,
            501,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        assert_eq!(scan["checkout_remaining"][0]["pid"], 12);
    }
    #[test]
    fn root_exemption_cannot_impersonate_the_stopped_assignment_pane() {
        let rows = vec![row(10, 1, 10, "ttys1", "zsh"), row(20, 1, 20, "?", "taskr")];
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            root: BTreeMap::from([(10, "zsh".into())]),
            root_pane: "w1:p1".into(),
            root_group: 10,
            root_at: "2026-01-01T00:00:00Z".into(),
        };
        assert!(
            evidence(
                &rows,
                &BTreeMap::new(),
                Path::new("/checkout"),
                10,
                20,
                501,
                &BTreeSet::new(),
                &trust
            )
            .is_err()
        );
    }
    #[test]
    fn zombies_need_no_cwd_and_are_not_remaining_writers() {
        let mut zombie = row(12, 10, 10, "ttys1", "zombie");
        zombie.zombie = true;
        let rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            zombie,
        ];
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            ..Default::default()
        };
        let scan = evidence(
            &rows,
            &BTreeMap::new(),
            Path::new("/checkout"),
            10,
            20,
            501,
            &BTreeSet::new(),
            &trust,
        )
        .unwrap();
        assert_eq!(scan["session_remaining"], json!([]));
        assert_eq!(scan["checkout_remaining"], json!([]));
        assert_eq!(scan["exempt"][0]["session_pids"], json!([10]));
    }
    #[test]
    fn lsof_exit_one_and_warnings_require_complete_live_pid_coverage() {
        let mut warning = process::Capture::default();
        warning.bytes = b"synthetic lsof warning".to_vec();
        let out = process::Output {
            pid: 51,
            exit: Some(1),
            signal: None,
            timed_out: false,
            cancelled: false,
            stdout: process::Capture::default(),
            stderr: warning,
        };
        assert!(
            parse_lsof(&probe_bytes(out, true).unwrap())
                .unwrap()
                .is_empty()
        );
        // Missing records are acceptable only for exact validated exemptions or zombies.
        let rows = vec![
            row(10, 1, 10, "ttys1", "zsh"),
            row(20, 1, 20, "?", "taskr"),
            row(51, 1, 51, "?", "writer"),
        ];
        let trust = Trust {
            shells: BTreeMap::from([(10, shell("zsh", "w1:p1", 10))]),
            ..Default::default()
        };
        assert!(
            evidence(
                &rows,
                &BTreeMap::new(),
                Path::new("/checkout"),
                10,
                20,
                501,
                &BTreeSet::new(),
                &trust
            )
            .is_err()
        );
        assert!(
            evidence(
                &rows,
                &BTreeMap::from([(51, PathBuf::from("/outside"))]),
                Path::new("/checkout"),
                10,
                20,
                501,
                &BTreeSet::new(),
                &trust
            )
            .is_ok()
        );
        assert!(deadline(Instant::now() - BUDGET).is_err());
        assert!(
            fixture(
                vec![row(51, 1, 51, "?", "ps")],
                "/outside",
                BTreeSet::from([51])
            )
            .is_err()
        );
    }
}
