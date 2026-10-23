#!/usr/bin/env python3
"""Bounded TLC safety plus public CLI replay. No downloads; see README.md."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import time
import zipfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
CONSUMER = "products/taskr/crates/taskr-core/src/store/consumer.rs"
INBOX = "products/taskr/crates/taskr-core/src/store/inbox.rs"
MUTATIONS = {
    "NC1": (CONSUMER, "if l.fence != fence {", "if false {", "AckAuthority"),
    "NC2": (CONSUMER, "old.map_or(0, |l| l.fence) + 1",
            "old.map_or(1, |l| l.fence + if force { 0 } else { 1 })", "AckAuthority"),
    "NC3": (INBOX, 'if kind == "got" {',
            'if kind == "got" || (filtered && kind == "ask") {', "RequiredHandled"),
}
WITNESSES = ("forcedTakeover", "fencedAck", "plainAck", "replay", "expiryReject",
             "generationReject", "staleReject", "filterAck", "coalesce", "crash",
             "renew", "release")
CASES = ("SYNC", "DOCTOR", "MODEL-BASELINE",
         *(f"MODEL-{n}" for n in MUTATIONS),
         *(f"REACH-{w}" for w in WITNESSES),
         "CLI-STALE", "CLI-REQUIRED", "CLI-EXPIRY", "CLI-GENERATION",
         *(f"RUST-{n}" for n in MUTATIONS))


class Blocked(Exception):
    pass


def sha(path):
    with Path(path).open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


class Run:
    def __init__(self, out, timeout):
        self.out, self.timeout = out, timeout
        self.results = {name: {"outcome": "blocked", "reason": "not executed"}
                        for name in CASES}
        self.commands = []

    def command(self, argv, cwd, env=None, timeout=None):
        argv = list(map(str, argv))
        number = len(self.commands) + 1
        log = self.out / f"command-{number:03d}.log"
        entry = {"argv": argv, "cwd": str(cwd), "log": log.name}
        self.commands.append(entry)
        started = time.monotonic()
        try:
            p = subprocess.Popen(argv, cwd=cwd, env=env, text=True, start_new_session=True,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                output, stderr = p.communicate(timeout=timeout or self.timeout)
                entry["exit"] = p.returncode
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                os.killpg(p.pid, signal.SIGKILL)
                output, stderr = p.communicate()
                entry["exit"] = None
                entry["timeout_or_interrupt"] = True
        except OSError as e:
            output = str(e)
            stderr = ""
            entry["exit"] = None
        entry["seconds"] = round(time.monotonic() - started, 3)
        log.write_text(output + stderr)
        entry["sha256"] = sha(log)
        write_json(self.out / "commands.json", self.commands)
        return entry["exit"], output

    def case(self, name, fn):
        start = len(self.commands)
        try:
            details = fn() or {}
            self.results[name] = {"outcome": "pass", **details}
        except Blocked as e:
            self.results[name] = {"outcome": "blocked", "reason": str(e)}
        except Exception as e:
            self.results[name] = {"outcome": "fail", "reason": str(e) or type(e).__name__}
        self.results[name]["commands"] = list(range(start + 1, len(self.commands) + 1))
        self.save()
        return self.results[name]["outcome"] == "pass"

    def save(self):
        outcomes = [v["outcome"] for v in self.results.values()]
        overall = "fail" if "fail" in outcomes else "blocked" if "blocked" in outcomes else "pass"
        write_json(self.out / "result.json", {"outcome": overall, "cases": self.results})


def check_sync(manifest, root):
    drift = [p for p, expected in manifest["source_files"].items()
             if not (root / p).is_file() or sha(root / p) != expected]
    if drift:
        raise Blocked("source drift; review action map before updating hashes: " + ", ".join(drift))
    return {"source_commit": manifest["source_commit"],
            "covered_files": len(manifest["source_files"])}


def check_package(manifest, run):
    drift = [p for p, expected in manifest["verification_files"].items()
             if not (HERE / p).is_file() or sha(HERE / p) != expected]
    if drift:
        raise Blocked("verification package drift; review before maintaining manifest: " + ", ".join(drift))
    result = check_sync(manifest, ROOT)
    code, head = run.command(["git", "rev-parse", "HEAD"], ROOT)
    if code != 0:
        raise Blocked("cannot identify the editing checkout HEAD")
    result["checkout_head"] = head.strip()
    return result


def executable(value, label):
    path = Path(value).expanduser().resolve() if value else None
    if path is None or not path.is_file() or not os.access(path, os.X_OK):
        raise Blocked(f"missing {label}: supply an existing executable path")
    return path


def doctor(run, args):
    missing = []
    for key in ("taskr", "java"):
        try:
            setattr(args, key, executable(getattr(args, key), key))
        except Blocked as e:
            missing.append(str(e))
    jar = Path(args.tlc_jar).expanduser().resolve() if args.tlc_jar else None
    if jar is None or not jar.is_file() or not zipfile.is_zipfile(jar):
        missing.append("missing TLC jar: supply --tlc-jar (no automatic download)")
    else:
        with zipfile.ZipFile(jar) as archive:
            if "tlc2/TLC.class" not in archive.namelist():
                missing.append("TLC jar does not contain tlc2.TLC")
        args.tlc_jar = jar
    if missing:
        raise Blocked("; ".join(missing))
    code, java = run.command([args.java, "-version"], run.out)
    if code != 0:
        raise Blocked("java -version failed; see command log")
    java = (run.out / run.commands[-1]["log"]).read_text()
    prerequisites = {}
    try:
        mutation_source(args, json.loads((HERE / "manifest.json").read_text()))
        executable(args.cargo, "cargo for offline mutation builds")
        prerequisites["real_mutants"] = "ready (offline dependency cache checked by build only)"
    except Blocked as e:
        prerequisites["real_mutants"] = "blocked: " + str(e)
    return {"java_version": java.strip(), "java_sha256": sha(args.java),
            "tlc_jar_sha256": sha(jar), "taskr_sha256": sha(args.taskr),
            "prerequisites": prerequisites,
            "tool": "tlc2.TLC exhaustive BFS, one worker; exact binaries pinned in this receipt"}


def tlc(run, args, name, mutation="baseline", witness="none"):
    folder = run.out / name
    folder.mkdir()
    shutil.copyfile(HERE / "Inbox.tla", folder / "Inbox.tla")
    config = (HERE / "Inbox.cfg").read_text().replace('Mutation = "baseline"',
                                                      f'Mutation = "{mutation}"')
    config = config.replace('WitnessCase = "none"', f'WitnessCase = "{witness}"')
    invariant = "WitnessNever" if witness != "none" else MUTATIONS.get(mutation, (None,) * 4)[3]
    if witness != "none":
        config += "INVARIANT WitnessNever\n"
    (folder / "Inbox.cfg").write_text(config)
    code, output = run.command([args.java, "-XX:+UseParallelGC", "-cp", args.tlc_jar,
                               "tlc2.TLC", "-workers", "1", "-seed", "1",
                               "-config", "Inbox.cfg", "Inbox"], folder)
    counts = re.findall(r"([\d,]+) states generated, ([\d,]+) distinct states found, "
                        r"([\d,]+) states left on queue", output)
    if code is None:
        raise Blocked("TLC did not finish; no counterexample credit")
    if not counts:
        raise AssertionError("missing TLC state counts; parser/tool failure is not evidence")
    generated, distinct, queued = [int(v.replace(",", "")) for v in counts[-1]]
    assert distinct > 1 and generated > 1, "vacuous state exploration"
    details = {"generated": generated, "distinct": distinct, "queued": queued,
               "config_sha256": sha(folder / "Inbox.cfg"),
               "model_sha256": sha(folder / "Inbox.tla")}
    if invariant is None:
        assert code == 0 and queued == 0, f"baseline did not finish successfully (exit {code})"
        assert "Model checking completed. No error has been found." in output, "no success marker"
    else:
        assert code == 12, f"expected safety violation exit 12, got {code}"
        assert f"Invariant {invariant} is violated." in output, f"wrong violation; expected {invariant}"
        assert "State 1:" in output and "State 2:" in output, "no multi-state counterexample"
        details["counterexample_invariant"] = invariant
        details["trace"] = run.commands[-1]["log"]
    return details


class CLI:
    def __init__(self, run, binary, folder):
        self.run, self.binary, self.folder = run, binary, folder
        folder.mkdir()
        self.db = folder / "taskr.db"
        (folder / "bin").mkdir()
        herdr = folder / "bin/herdr"
        herdr.write_text("#!/bin/sh\nexit 1\n")
        herdr.chmod(0o755)
        self.env = {"HOME": str(folder), "XDG_STATE_HOME": str(folder / "state"),
                    "TASKR_DB": str(self.db), "TASKR_TMP_BASE": str(folder / "tmp"),
                    "TMPDIR": str(folder), "PATH": str(folder / "bin") + ":/usr/bin:/bin",
                    "HERDR_SOCKET_PATH": str(folder / "absent.sock"),
                    "TASKR_FROZEN_NOW": "2026-01-01T00:00:00.000Z"}
        self.root = str(self.call("new", "fixture", "--role", "orchestrator")["task_id"])
        brief = folder / "brief.md"
        brief.write_text("Synthetic inbox verification fixture. No agent is started.\n")
        self.worker = str(self.call("new", "fixture-worker", "--role", "implementer",
                                    "--parent", self.root, "--brief", brief,
                                    "--cwd", folder)["task_id"])
        launch = self.call("launch", self.worker, "--provider", "codex", "--model", "fixture",
                           "--effort", "high")["launch_id"]
        self.worker_env = {"TASKR_TASK": self.worker, "TASKR_LAUNCH": str(launch)}
        report = folder / "report.md"
        report.write_text("Synthetic ready event; ASK_ONLY deliberately excludes this kind.\n")
        self.ready = self.call("ready", "fixture", "--report", report,
                               worker=True)["event_id"]
        self.ask = self.call("ask", "fixture question", worker=True)["ask_id"]

    def call(self, *args, worker=False, expected=0, raw=False):
        env = {**self.env, **(self.worker_env if worker else {})}
        code, text = self.run.command([self.binary, "--json", *args], self.folder, env)
        if code is None:
            raise Blocked("CLI command did not complete; see command log")
        assert code == expected, f"CLI exit {code}, expected {expected}; see command log"
        if raw:
            return text
        # JSON wait may emit an initial skipped-count object before the event.
        decoder, rows = json.JSONDecoder(), []
        remaining = text.strip()
        while remaining:
            value, end = decoder.raw_decode(remaining)
            rows.append(value)
            remaining = remaining[end:].lstrip()
        assert rows, "CLI returned no JSON"
        return rows[-1]

    def take(self, holder, force=False):
        args = ["consumer", "take", "--as", self.root, "--session", "0",
                "--holder", holder, "--ttl", "60000"]
        if force:
            args.append("--force")
        return self.call(*args)

    def wait(self, fence, expected=0):
        return self.call("wait", "--as", self.root, "--session", "0", "--fence", str(fence),
                         "--for", "ask", "--timeout", "0", expected=expected)

    def ack(self, fence, expected=0, session="0"):
        return self.call("ack", str(self.ask), "--as", self.root, "--session", session,
                         "--fence", str(fence), expected=expected)

    def snapshot(self):
        with sqlite3.connect(f"file:{self.db}?mode=ro", uri=True) as db:
            return {"cursor": db.execute("select acked_event_id,pending_event_id from tasks "
                                         "where id=?", (self.root,)).fetchone(),
                    "lease": db.execute("select * from consumer_leases where task_id=?",
                                        (self.root,)).fetchone()}


def replay(run, binary, case, mutant=None):
    folder = run.out / case
    cli = CLI(run, binary, folder)
    first = cli.take("consumer-a")
    assert first["fence"] == 1 and first["live"] is True, "initial live fence missing"
    if case == "CLI-EXPIRY" and first["expires_at"] != "2026-01-01T00:01:00.000Z":
        raise Blocked("--taskr must be built with --features contract for deterministic expiry")
    if mutant == "NC3":
        result = cli.wait(1, expected=3)
        assert result["timeout"] is True, "mutated ask was not skipped"
        assert cli.snapshot()["cursor"] == (cli.ask, None), "required event was not lost"
        return {"counterexample": "required ask coalesced without handling", "event_id": cli.ask}
    offered = cli.wait(1)
    assert offered["event"]["id"] == cli.ask, "required ask not offered"
    assert cli.snapshot()["cursor"] == (cli.ready, cli.ask), "filter crossed required ask"
    if case == "CLI-REQUIRED":
        again = cli.wait(1)
        assert again["event"]["id"] == cli.ask and again["replay"] is True, "pending ask not replayed"
        # Handling the fixture question is external to the store, as in Handle(c).
        (folder / "handled.json").write_text(json.dumps({"handled_event_id": cli.ask}) + "\n")
        cli.ack(1)
        assert cli.snapshot()["cursor"] == (cli.ask, None), "handled ask did not commit"
    elif case == "CLI-EXPIRY":
        cli.env["TASKR_FROZEN_NOW"] = "2026-01-01T00:01:00.000Z"
        before = cli.snapshot()
        rejected = cli.ack(1, expected=6)
        assert "not live" in rejected["error"], "expiry rejection not identified"
        assert cli.snapshot() == before, "expired ack wrote state"
    elif case == "CLI-GENERATION":
        cli.call("adopt", cli.root, "--pane", "w1:p1", raw=True)
        before = cli.snapshot()
        rejected = cli.ack(1, expected=6)
        assert "session generation" in rejected["error"], "old generation not identified"
        assert cli.snapshot() == before, "old-generation ack wrote state"
    else:
        (folder / "handled-before-takeover.json").write_text(
            json.dumps({"handled_event_id": cli.ask}) + "\n")
        second = cli.take("consumer-b", force=True)
        assert second["live"] is True, "forced takeover was not live"
        assert second["fence"] == (1 if mutant == "NC2" else 2), "wrong takeover fence"
        before = cli.snapshot()
        if mutant:
            cli.ack(1)
            assert cli.snapshot()["cursor"] == (cli.ask, None), "stale ack did not commit"
            return {"counterexample": "old consumer committed after live forced takeover",
                    "event_id": cli.ask, "old_fence": 1, "new_fence": second["fence"]}
        rejected = cli.ack(1, expected=6)
        assert "is stale" in rejected["error"], "stale fence rejection not identified"
        assert cli.snapshot() == before, "stale ack changed lease or cursor"
        again = cli.wait(2)
        assert again["event"]["id"] == cli.ask and again["replay"] is True, "successor lost pending ask"
        (folder / "handled.json").write_text(json.dumps({"handled_event_id": cli.ask}) + "\n")
        cli.ack(2)
        assert cli.snapshot()["cursor"] == (cli.ask, None), "successor failed to commit"
    write_json(folder / "snapshot.json", cli.snapshot())
    return {"required_event": cli.ask, "excluded_event": cli.ready,
            "database": str(cli.db.relative_to(run.out))}


def mutation_source(args, manifest):
    if not args.mutation_source:
        raise Blocked("real mutation replay requires --mutation-source: a disposable source copy "
                      "with .inbox-proof-disposable containing the manifest source_commit")
    source = Path(args.mutation_source).expanduser().resolve()
    if source == ROOT or source in ROOT.parents or ROOT in source.parents:
        raise Blocked("mutation source must be outside the editing checkout")
    marker = source / ".inbox-proof-disposable"
    if not marker.is_file() or marker.read_text().strip() != manifest["source_commit"]:
        raise Blocked("missing disposable-copy marker or wrong source commit")
    check_sync(manifest, source)
    return source


def rust_mutants(run, args, manifest):
    source = mutation_source(args, manifest)
    for name, (rel, old, new, _) in MUTATIONS.items():
        path = source / rel
        if path.is_symlink() or any(p.is_symlink() for p in path.parents):
            raise Blocked("mutation source paths must not be symlinks")
        if path.stat().st_nlink != 1:
            raise Blocked("mutation source paths must not be hardlinked")
        original = path.read_bytes()
        if original.decode().count(old) != 1:
            raise Blocked(f"{name}: mutation anchor drifted")
        def build_and_replay():
            try:
                path.write_text(original.decode().replace(old, new))
                write_json(run.out / f"{name}-mutation.json", {
                    "path": rel, "before_sha256": hashlib.sha256(original).hexdigest(),
                    "after_sha256": sha(path), "old": old, "new": new})
                env = {**os.environ, "CARGO_NET_OFFLINE": "true", "RUSTUP_AUTO_INSTALL": "0"}
                code, _ = run.command([args.cargo, "build", "--offline", "--locked", "-p", "taskr",
                                       "--features", "contract", "--target-dir", run.out / "build"],
                                      source, env, timeout=args.build_timeout)
                if code != 0:
                    raise Blocked(f"{name} offline mutation build failed; no negative-control credit")
                binary = run.out / f"taskr-{name}"
                shutil.copy2(run.out / "build/debug/taskr", binary)
                return {"binary_sha256": sha(binary), **replay(run, binary, f"RUST-{name}", name)}
            finally:
                path.write_bytes(original)
        run.case(f"RUST-{name}", build_and_replay)


def main():
    if not __debug__:
        raise SystemExit("verification requires assertions: run Python without -O or PYTHONOPTIMIZE")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--taskr")
    parser.add_argument("--tlc-jar")
    parser.add_argument("--java")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--doctor", action="store_true", help="check source and prerequisites only")
    parser.add_argument("--mutation-source")
    parser.add_argument("--cargo", default=shutil.which("cargo"))
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--build-timeout", type=int, default=900)
    args = parser.parse_args()
    if args.timeout <= 0 or args.build_timeout <= 0:
        parser.error("timeouts must be positive seconds")
    args.out = args.out.expanduser().resolve()
    if args.out == ROOT or ROOT in args.out.parents or args.out in ROOT.parents:
        parser.error("--out must be a fresh directory outside the checkout")
    args.out.mkdir(parents=True, exist_ok=False)
    run = Run(args.out, args.timeout)
    manifest = json.loads((HERE / "manifest.json").read_text())
    write_json(args.out / "manifest.json", manifest)
    write_json(args.out / "package-hashes.json", {p.name: sha(p) for p in HERE.iterdir() if p.is_file()})
    try:
        synced = run.case("SYNC", lambda: check_package(manifest, run))
        healthy = run.case("DOCTOR", lambda: doctor(run, args))
        if not args.doctor and synced and healthy:
            run.case("MODEL-BASELINE", lambda: tlc(run, args, "MODEL-BASELINE"))
            for name in MUTATIONS:
                run.case(f"MODEL-{name}", lambda n=name: tlc(run, args, f"MODEL-{n}", mutation=n))
            for witness in WITNESSES:
                run.case(f"REACH-{witness}", lambda w=witness: tlc(run, args, f"REACH-{w}", witness=w))
            for case in ("CLI-STALE", "CLI-REQUIRED", "CLI-EXPIRY", "CLI-GENERATION"):
                run.case(case, lambda c=case: replay(run, args.taskr, c))
            try:
                args.cargo = executable(args.cargo, "cargo for offline mutation builds")
                rust_mutants(run, args, manifest)
            except (Blocked, OSError) as e:
                for name in MUTATIONS:
                    if run.results[f"RUST-{name}"].get("reason") == "not executed":
                        run.results[f"RUST-{name}"] = {"outcome": "blocked", "reason": str(e)}
    finally:
        run.save()
        # Keep evidence and synthetic SQLite ledgers. No live profiles or processes.
        files = {str(p.relative_to(args.out)): sha(p) for p in args.out.rglob("*")
                 if p.is_file() and "build" not in p.relative_to(args.out).parts
                 and "states" not in p.relative_to(args.out).parts and p.name != "evidence-hashes.json"}
        write_json(args.out / "evidence-hashes.json", files)
    result = json.loads((args.out / "result.json").read_text())
    if args.doctor:
        ready = healthy and run.results["DOCTOR"]["prerequisites"]["real_mutants"].startswith("ready")
        outcome = "pass" if synced and ready else "blocked"
        print(f"doctor: {outcome}; proof cases unrun; {args.out / 'result.json'}")
        for name in ("SYNC", "DOCTOR"):
            print(f"{name}: {json.dumps(run.results[name], sort_keys=True)}")
        return 0 if outcome == "pass" else 2
    print(f"{result['outcome']}: {args.out / 'result.json'}")
    return {"pass": 0, "fail": 1, "blocked": 2}[result["outcome"]]


if __name__ == "__main__":
    sys.exit(main())
