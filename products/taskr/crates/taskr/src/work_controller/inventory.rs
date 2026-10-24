//! Cooperative same-user evidence, never authority to kill an unexplained process.
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
fn parse_ps(bytes: &[u8]) -> Result<Vec<Row>> {
    let text = std::str::from_utf8(bytes).map_err(failure)?;
    if text.is_empty() || !text.ends_with('\n') {
        return Err(failure("incomplete ps output"));
    }
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 6 || rows.len() == MAX {
            return Err(failure("invalid or bounded ps inventory"));
        }
        let id = pid(fields[0], false)?;
        let command = fields[5..].join(" ");
        if !seen.insert(id)
            || command.len() > TEXT
            || command.chars().any(char::is_control)
            || fields[3].len() > 128
            || fields[4].len() > 16
            || !matches!(
                fields[4].chars().next(),
                Some('D' | 'I' | 'R' | 'S' | 'T' | 'U' | 'W' | 'X' | 'Z' | 't')
            )
        {
            return Err(failure("duplicate or invalid ps row"));
        }
        rows.push(Row {
            pid: id,
            parent: pid(fields[1], true)?,
            group: pid(fields[2], false)?,
            tty: fields[3].into(),
            zombie: fields[4].starts_with('Z'),
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
    // SAFETY: kill with signal zero only checks the exact PID; it sends no signal.
    if unsafe { libc::kill(id, 0) } == 0 {
        return Ok(true);
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(failure(e))
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
            "-U",
            &uid.to_string(),
            "-o",
            "pid=,ppid=,pgid=,tty=,stat=,comm=",
        ],
        start,
        probes,
    )?;
    let mut rows = Vec::new();
    for mut row in parse_ps(&bytes)? {
        deadline(start)?;
        // SAFETY: getsid reads the true POSIX session, including on macOS where ps sess is zero.
        row.session = unsafe { libc::getsid(row.pid) };
        if row.session <= 0 {
            if !alive(row.pid)? {
                continue;
            }
            return Err(failure("cannot read live process session"));
        }
        rows.push(row);
    }
    Ok(rows)
}
fn same_command(a: &str, b: &str) -> bool {
    Path::new(a).file_name() == Path::new(b).file_name()
}
fn tty(tty: &str) -> bool {
    !matches!(tty, "?" | "??" | "-")
}
#[allow(clippy::too_many_arguments)] // Exact OS observations and trust inputs are independently fixture-controlled.
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
    let mut exempt = BTreeMap::from([(
        observer,
        json!({"pid":observer,"reason":"observer","observer_pid":observer,"parent_chain":[observer]}),
    )]);
    for r in rows {
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
        if r.zombie || probes.contains(&r.pid) {
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
    if session.len() > 1024 || checkout_remaining.len() > 1024 || exempt.len() > 1024 {
        return Err(failure("process evidence array bound exceeded"));
    }
    Ok(
        json!({"method":"local-user-session-cwd-v1","uid":uid,"shell_pid":shell,"session_id":target.session,
        "scanned":rows.len(),"session_remaining":session,"checkout_remaining":checkout_remaining,
        "exempt":exempt.into_values().collect::<Vec<_>>()}),
    )
}
pub(super) fn scan(checkout: &Path, shell: i32, trust: &Trust) -> Result<Value> {
    let start = Instant::now();
    // SAFETY: getuid has no arguments or failure mode.
    let uid = unsafe { libc::getuid() };
    let observer = i32::try_from(std::process::id()).map_err(failure)?;
    let checkout = fs::canonicalize(checkout).map_err(failure)?;
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
    for r in &first {
        deadline(start)?;
        if r.zombie || probes.contains(&r.pid) {
            continue;
        }
        #[cfg(target_os = "linux")]
        let path = fs::read_link(format!("/proc/{}/cwd", r.pid));
        #[cfg(target_os = "macos")]
        let path = lsof
            .get(&r.pid)
            .cloned()
            .ok_or_else(|| std::io::Error::other("missing lsof cwd"));
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let path: std::io::Result<PathBuf> =
            Err(std::io::Error::other("unsupported process inventory OS"));
        match path.and_then(fs::canonicalize) {
            Ok(path) if path.as_os_str().len() <= TEXT => {
                cwds.insert(r.pid, path);
            }
            _ if !alive(r.pid)? => {}
            _ => {} // Only a validated exact exemption can account for this missing cwd below.
        }
    }
    let last = snapshot(uid, start, &mut probes)?;
    for old in &first {
        deadline(start)?;
        if !probes.contains(&old.pid) && !last.iter().any(|r| r.pid == old.pid) && alive(old.pid)? {
            return Err(failure(
                "process omitted from final inventory is still alive",
            ));
        }
    }
    for r in &last {
        if probes.contains(&r.pid) {
            continue;
        }
        if !first.iter().any(|old| old == r) {
            return Err(failure("process inventory changed during scan"));
        }
    }
    deadline(start)?;
    let scan = evidence(
        &last, &cwds, &checkout, shell, observer, uid, &probes, trust,
    )?;
    deadline(start)?;
    Ok(scan)
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
        assert_eq!(parse_ps(b" 10 1 10 ttys1 S /bin/zsh\n").unwrap()[0].pid, 10);
        for bad in [
            b"".as_slice(),
            b"10 1 10 ttys1 S zsh",
            b"10 bad 10 ttys1 S zsh\n",
            b"0 1 1 ? S zsh\n",
            b"10 1 10 ? S\n",
            b"10 1 10 ? S zsh\n10 1 10 ? S zsh\n",
        ] {
            assert!(parse_ps(bad).is_err());
        }
        let large = (1..=MAX + 1)
            .map(|id| format!("{id} 1 {id} ? S taskr\n"))
            .collect::<String>();
        assert!(parse_ps(large.as_bytes()).is_err());
        let valid = b"p10\0czsh\0\nfcwd\0n/checkout with spaces\0\np20\0ctaskr\0\nfcwd\0n/\0\n";
        assert_eq!(
            parse_lsof(valid).unwrap()[&10],
            Path::new("/checkout with spaces")
        );
        for bad in [
            b"p10\0czsh\0\nfcwd\0n/a",
            b"p10\0czsh\0\nfcwd\0nrelative\0\n",
            b"p10\0czsh\0\nfcwd\0n/a\0\np10\0czsh\0\nfcwd\0n/b\0\n",
            b"p10\0czsh\0\np20\0ctaskr\0\nfcwd\0n/a\0\n",
            b"p10\0czsh\0\nfcwd\0n/a\0\nxunexpected\0\n",
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
