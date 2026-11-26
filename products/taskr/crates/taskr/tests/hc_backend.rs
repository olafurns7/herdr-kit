//! `HERDR_KIT_BACKEND=hc`: the Herdr guard checks hc's core socket, `herdr` on
//! PATH is the shim, and herdr 0.9's socket is never connected.
#[allow(dead_code)]
#[path = "hook/support.rs"]
mod support;

use serde_json::Value;
use std::{
    fs,
    io::Read,
    os::unix::net::UnixListener,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::*;

/// A listening socket that counts connections and never answers.
fn counting(path: &Path) -> Arc<AtomicUsize> {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(path).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            seen.fetch_add(1, Ordering::SeqCst);
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.read_to_end(&mut Vec::new());
            });
        }
    });
    count
}

/// Late accepts land before the count is read.
fn settled(count: &AtomicUsize) -> usize {
    std::thread::sleep(Duration::from_millis(200));
    count.load(Ordering::SeqCst)
}

/// A fake `herdr` that records argv and the socket variables it saw.
fn fake_herdr(h: &Harness) {
    h.script(
        "herdr",
        r#"#!/usr/bin/python3
import json, os, pathlib, sys
home = pathlib.Path(os.environ['HOME'])
seen = {k: os.environ.get(k) for k in ('HERDR_SOCKET_PATH', 'HERDR_KIT_BACKEND')}
with (home / 'herdr-calls').open('a') as log:
    log.write(json.dumps({'argv': sys.argv[1:], 'env': seen}) + '\n')
if sys.argv[1:3] == ['agent', 'list']:
    print('{"result":{"agents":[]}}')
else:
    print('{"result":{"agent":{"agent_status":"working"}}}')
"#,
    );
}

fn calls(h: &Harness) -> Vec<Value> {
    fs::read_to_string(h.home.join("herdr-calls"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn starts(call: &Value, a: &str, b: &str) -> bool {
    call["argv"][0] == a && call["argv"][1] == b
}

fn env(pairs: &[(&str, &Path)]) -> Env {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
        .collect()
}

fn with(mut base: Env, key: &str, value: &str) -> Env {
    base.push((key.into(), value.into()));
    base
}

#[test]
fn prompt_under_hc_checks_the_core_socket_and_runs_the_shim() {
    let h = Harness::new();
    fake_herdr(&h);
    let (task, launch, _) = h.lane("codex", "implementer");
    let herdr = counting(&h.home.join("herdr.sock"));
    let hc = counting(&h.home.join("hc.sock"));
    let vars = with(
        env(&[
            ("HERDR_SOCKET_PATH", &h.home.join("herdr.sock")),
            ("HC_SOCKET", &h.home.join("hc.sock")),
        ]),
        "HERDR_KIT_BACKEND",
        "hc",
    );
    let out = h.ok(&vars, &["prompt", &task.to_string(), "--text", "work"]);
    assert_eq!(out["outcome"], "activity_observed", "{out}");
    assert_eq!(settled(&herdr), 0, "herdr 0.9's socket was connected");
    assert!(
        hc.load(Ordering::SeqCst) >= 1,
        "hc's socket was not checked"
    );
    let calls = calls(&h);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(starts(&calls[0], "agent", "prompt"), "{calls:?}");
    assert_eq!(calls[0]["env"]["HERDR_SOCKET_PATH"], Value::Null);
    assert_eq!(calls[0]["env"]["HERDR_KIT_BACKEND"], "hc");
    // Capacity needs herdr's socket API; under hc it stays unknown.
    assert_eq!(
        h.count("select count(*) from meta where key='capacity:'||?", launch),
        0
    );
}

#[test]
fn hc_socket_defaults_to_the_runtime_dir_and_session() {
    let h = Harness::new();
    fake_herdr(&h);
    let (task, _, _) = h.lane("claude", "implementer");
    let run = h.home.join("run");
    let hc = counting(&run.join("herdr-core/s1.sock"));
    let vars = with(
        with(
            env(&[
                ("HERDR_SOCKET_PATH", &h.home.join("absent.sock")),
                ("XDG_RUNTIME_DIR", &run),
            ]),
            "HC_SESSION",
            "s1",
        ),
        "HERDR_KIT_BACKEND",
        "hc",
    );
    let out = h.ok(&vars, &["prompt", &task.to_string(), "--text", "work"]);
    assert_eq!(out["outcome"], "activity_observed", "{out}");
    assert!(hc.load(Ordering::SeqCst) >= 1);
}

#[test]
fn no_hc_core_runs_no_herdr_even_with_herdr_up() {
    let h = Harness::new();
    fake_herdr(&h);
    let (task, _, _) = h.lane("codex", "implementer");
    let herdr = counting(&h.home.join("herdr.sock"));
    let vars = with(
        env(&[
            ("HERDR_SOCKET_PATH", &h.home.join("herdr.sock")),
            ("HC_SOCKET", &h.home.join("absent.sock")),
        ]),
        "HERDR_KIT_BACKEND",
        "hc",
    );
    let out = h.output(
        &vars,
        &["--json", "prompt", &task.to_string(), "--text", "work"],
        b"",
    );
    assert!(!out.status.success(), "{out:?}");
    let err: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        err["error"]
            .as_str()
            .unwrap()
            .contains("Herdr server not reachable at")
            && err["error"].as_str().unwrap().contains("absent.sock"),
        "{err}"
    );
    assert!(calls(&h).is_empty());
    assert_eq!(settled(&herdr), 0);
}

#[test]
fn daemon_pass_under_hc_lists_agents_through_the_shim() {
    let h = Harness::new();
    fake_herdr(&h);
    let _ = h.lane("codex", "implementer");
    let herdr = counting(&h.home.join("herdr.sock"));
    let _hc = counting(&h.home.join("hc.sock"));
    let vars = with(
        env(&[
            ("HERDR_SOCKET_PATH", &h.home.join("herdr.sock")),
            ("HC_SOCKET", &h.home.join("hc.sock")),
        ]),
        "HERDR_KIT_BACKEND",
        "hc",
    );
    let out = h.ok(&vars, &["daemon", "--once"]);
    assert_eq!(out["ok"], true, "{out}");
    let calls = calls(&h);
    assert!(
        calls.iter().any(|c| starts(c, "agent", "list")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|c| c["env"]["HERDR_SOCKET_PATH"].is_null()),
        "{calls:?}"
    );
    assert_eq!(settled(&herdr), 0);
}

/// Unset or any other value keeps herdr 0.9: its socket is checked and passed
/// on, and a Codex prompt asks its socket API for capacity.
#[test]
fn other_backend_values_keep_herdr() {
    for backend in [None, Some("herdr")] {
        let h = Harness::new();
        fake_herdr(&h);
        let (task, _, _) = h.lane("codex", "implementer");
        let herdr = counting(&h.home.join("herdr.sock"));
        let hc = counting(&h.home.join("hc.sock"));
        let mut vars = env(&[
            ("HERDR_SOCKET_PATH", &h.home.join("herdr.sock")),
            ("HC_SOCKET", &h.home.join("hc.sock")),
        ]);
        if let Some(value) = backend {
            vars = with(vars, "HERDR_KIT_BACKEND", value);
        }
        let out = h.ok(&vars, &["prompt", &task.to_string(), "--text", "work"]);
        assert_eq!(out["outcome"], "activity_observed", "{out}");
        assert!(settled(&herdr) >= 2, "guard and capacity bind");
        assert_eq!(hc.load(Ordering::SeqCst), 0);
        let calls = calls(&h);
        assert_eq!(
            calls[0]["env"]["HERDR_SOCKET_PATH"],
            h.home.join("herdr.sock").to_string_lossy().as_ref()
        );
    }
}
