"""Small, isolated checks for the native identity boundary and crash receipts."""
import importlib.machinery
import importlib.util
import json
import fcntl
import errno
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import Mock, patch

SOURCE = Path(__file__).resolve().parents[1] / "bin/herdr-codex-identity"
loader = importlib.machinery.SourceFileLoader("identity", str(SOURCE))
spec = importlib.util.spec_from_loader(loader.name, loader)
m = importlib.util.module_from_spec(spec)
loader.exec_module(m)
ID = "01900000-0000-7000-8000-000000000001"
OTHER = "01900000-0000-7000-8000-000000000002"


class Fake(m.Adapter):
    def __init__(self, root, fault=""):
        self.scratch = root
        cwd = root / "checkout"
        cwd.mkdir(exist_ok=True)
        taskr = root / "taskr"
        taskr.write_text("#!/bin/sh\nexit 0\n")
        taskr.chmod(0o700)
        with patch.dict(os.environ, TASKR_WORK_EXECUTABLE=str(taskr), TASKR_DB=str(root / "scratch.db")):
            super().__init__(["2", "3", "1", "4", "impl-w3", "w1:p1", str(cwd)])
        self.socket = str(root / "s")
        self.calls = []
        self.fault = fault
        self.native = None
        self.agent_status = "idle"
        self.interactive_ready = True
        self.process_version = 1
        self.sent_count = 0
        self.assignment = dict(launch_id=3, task_id=2, round=0, state="starting", process_state="starting",
                               stage="executor", native_session=None, lane=dict(provider="codex"),
                               dispatch=dict(workspace="w1", tab="w1:t1", pane="w1:p1"))
        self.snap = dict(campaign=1, generation=4, parked=False, configuration=dict(enabled=True, workspace="w1"),
                         work=[dict(work_id=6, current_run=5, revision=1,
                                    revisions=[dict(revision=1, contract=dict(checkout=dict(cwd=str(cwd))))],
                                    runs=[dict(run_id=5, revision=1, round=0, state="active", assignments=[self.assignment])])])
        self.task_state = dict(id=2, current_launch_id=3, status="open", role="implementer", agent_name=None,
                               workspace_id="w1", tab_id="w1:t1", pane_id="w1:p1")
        self.log = [dict(record="launch", id=3, task_id=2, provider="codex", pane_id="w1:p1", session_ref=None)]
        exe = root / "codex"
        exe.write_bytes(b"\x7fELF")
        self.exe = str(exe)
        self.folder = root / "artifacts"
        self.folder.mkdir(mode=0o700)

    def command(self, args):
        self.calls.append(args)
        if args[2:4] == ["work", "list"]:
            return m.encoded(self.snap)
        if args[2] == "status":
            return m.encoded(self.task_state)
        if args[2] == "log":
            return b"".join(m.encoded(v) for v in self.log)
        raise AssertionError(args)

    def process(self, pid):
        return dict(pid=pid, start=self.process_version, exe=self.exe, argv=[self.exe],
                    cwd=str(self.cwd), group=30)

    def preflight(self):
        if self.fault == "server-down":
            raise OSError("unavailable")

    def herdr(self, *args):
        self.calls.append(args)
        if args[:2] == ("agent", "list"):
            a = dict(name=self.name, pane_id=self.pane, agent="codex", agent_status=self.agent_status,
                     interactive_ready=self.interactive_ready,
                     workspace_id="w1", tab_id="w1:t1", cwd=str(self.cwd), foreground_cwd=str(self.cwd), terminal_id="term")
            if self.native:
                a["agent_session"] = dict(value=self.native, agent="codex", source="herdr:codex", kind="id")
            return dict(agents=[a, a] if self.fault == "duplicate" else [a])
        if args[:2] == ("pane", "get"):
            return dict(pane=dict(pane_id=self.pane, workspace_id="w1", tab_id="w1:t1",
                                  cwd=str(self.cwd), foreground_cwd=str(self.cwd)))
        if args[:2] == ("pane", "process-info"):
            return dict(process_info=dict(pane_id=self.pane, foreground_process_group_id=30,
                                          foreground_processes=[dict(pid=30)]))
        if args[:2] == ("agent", "prompt"):
            self.sent_count += 1
            if self.fault == "unknown-delivery":
                raise OSError("unknown delivery")
            import shlex
            command = shlex.split(args[3][2:])
            self.assert_command(command)
            env = dict(os.environ, TASKR_TASK="2", TASKR_LAUNCH="3", HERDR_ENV="1", HERDR_PANE_ID=self.pane,
                       CODEX_THREAD_ID=ID, CODEX_SESSION_ID=ID)
            if self.fault == "wrong-env":
                env["TASKR_LAUNCH"] = "99"
            result = subprocess.run(command, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3)
            if self.fault == "wrong-env":
                assert result.returncode != 0
                raise RuntimeError("probe environment refused")
            assert result.returncode == 0, result.stderr
            if self.fault == "replace-process":
                self.process_version += 1
            if self.fault == "generation-race":
                self.snap["generation"] += 1
            return dict(type="agent_prompted")
        if args[:2] == ("agent", "wait"):
            return {}
        raise AssertionError(args)

    def assert_command(self, command):
        assert command[:3] == [str(Path(sys.executable).resolve()), "-I", "-B"]
        assert command[3] == str(self.folder / "probe.py")

    def run_test(self):
        return self.initialize(self.folder)


class IdentityTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name).resolve()
        self.fake = Fake(self.root)
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.bind(self.fake.socket)
        self.socket.listen()
        self.socket.settimeout(2)
        self.requests = []
        self.errors = []

    def tearDown(self):
        self.socket.close()
        self.tmp.cleanup()

    def report_server(self, ignored=False):
        def run():
            try:
                with self.socket.accept()[0] as conn:
                    request = json.loads(conn.makefile("rb").readline(16385))
                    self.requests.append(request)
                    if not ignored:
                        self.fake.native = request["params"]["agent_session_id"]
                    conn.sendall(m.encoded(dict(id=request["id"], result=dict(type="ok"))))
            except Exception as e:
                self.errors.append(e)
        thread = threading.Thread(target=run)
        thread.start()
        return thread

    def test_shell_receipt_report_and_recovery_do_not_resend(self):
        for _ in range(2):
            thread = self.report_server()
            result = self.fake.run_test()
            thread.join(3)
            self.assertFalse(thread.is_alive())
            self.assertFalse(self.errors)
            self.assertEqual(result["native_session"], ID)
            provenance = json.loads(m.read_private(Path(result["provenance_path"])))
            self.assertEqual(provenance["method"], "native-shell-receipt")
            self.assertIs(provenance["hook_invoked"], False)
        self.assertEqual(self.fake.sent_count, 1)
        self.assertEqual(self.requests[0]["params"]["source"], "herdr:codex")
        self.assertNotIn("session_start_source", self.requests[0]["params"])
        self.assertEqual(self.fake.assignment["process_state"], "starting")

    def test_ok_without_native_metadata_is_refused(self):
        thread = self.report_server(ignored=True)
        with self.assertRaisesRegex(RuntimeError, "report ignored"):
            self.fake.run_test()
        thread.join(3)
        self.assertFalse(self.errors)

    def test_existing_metadata_does_not_send_a_command(self):
        self.fake.native = ID
        result = self.fake.run_test()
        self.assertEqual(result["native_session"], ID)
        self.assertEqual(self.fake.sent_count, 0)

    def completed_attempt(self, f):
        f.guard()
        init = dict(context=f.context, basis=f.basis, nonce="synthetic-completed-shell")
        m.write_once(f.folder / "init.json", m.encoded(init))
        m.write_once(f.folder / "probe.py", f.script(init, f.folder))
        m.write_once(f.folder / "sent.json", m.encoded(dict(nonce=init["nonce"])))
        self.write_shell_receipt(f, init)
        f.basis = None  # A new adapter process must recover the pinned basis from disk.
        f.completed_native = None
        f.agent_status = "done"

    def write_shell_receipt(self, f, init):
        value = dict(version=1, nonce=init["nonce"], TASKR_TASK="2", TASKR_LAUNCH="3", HERDR_ENV="1",
                     HERDR_PANE_ID=f.pane, CODEX_THREAD_ID=ID, CODEX_SESSION_ID=ID)
        m.write_once(f.folder / "receipt.json", m.encoded(value))

    def fake_report(self, native):
        self.fake.guard(native)
        self.fake.native = native
        self.fake.guard(native)

    def test_done_without_completed_receipt_cannot_initialize(self):
        self.fake.agent_status = "done"
        self.fake.native = ID
        with self.assertRaisesRegex(RuntimeError, "not ready"):
            self.fake.guard(ID)
        with self.assertRaisesRegex(RuntimeError, "not ready"):
            self.fake.run_test()
        self.assertEqual(self.fake.sent_count, 0)
        self.assertFalse((self.fake.folder / "init.json").exists())

    def test_fresh_shell_done_uses_default_wait_and_checks_receipt(self):
        original = self.fake.herdr
        def herdr(*args):
            if args[:2] != ("agent", "prompt"):
                return original(*args)
            self.fake.calls.append(args)
            self.fake.sent_count += 1
            init = json.loads(m.read_private(self.fake.folder / "init.json"))
            self.write_shell_receipt(self.fake, init)
            fd = os.open(self.fake.folder / "notify", os.O_WRONLY | os.O_NONBLOCK)
            try:
                os.write(fd, b"1")
            finally:
                os.close(fd)
            self.fake.agent_status = "done"
            return {}
        with patch.object(self.fake, "herdr", side_effect=herdr), \
                patch.object(self.fake, "report", side_effect=self.fake_report):
            result = self.fake.run_test()
        self.assertEqual(result["native_session"], ID)
        self.assertEqual(self.fake.sent_count, 1)
        waits = [a for a in self.fake.calls if a[:2] == ("agent", "wait")]
        self.assertEqual(len(waits), 1)
        self.assertNotIn("--until", waits[0])

    def test_done_completed_receipt_recovers_through_run_without_resend(self):
        with patch.dict(os.environ, HOME=str(self.root / "home")):
            base = Path.home() / ".local/state/taskr/native-identity"
            key = dict(db=self.fake.context["db"], launch=self.fake.launch)
            self.fake.folder = base / m.hashlib.sha256(m.encoded(key)).hexdigest()
            self.fake.folder.mkdir(mode=0o700, parents=True)
            base.chmod(0o700)
            self.completed_attempt(self.fake)
            with patch.object(self.fake, "report", side_effect=self.fake_report):
                result = self.fake.run()
        self.assertEqual(result["native_session"], ID)
        self.assertEqual(self.fake.sent_count, 0)
        self.assertFalse(any(a[:2] in (("agent", "prompt"), ("agent", "wait")) for a in self.fake.calls))
        self.assertEqual(self.fake.assignment["process_state"], "starting")

    def test_completed_receipt_does_not_relax_other_state_or_identity_guards(self):
        changes = [lambda f: setattr(f, "agent_status", "working"), lambda f: setattr(f, "agent_status", "blocked"),
                   lambda f: setattr(f, "agent_status", "uncertain"), lambda f: setattr(f, "interactive_ready", False),
                   lambda f: setattr(f, "native", OTHER), lambda f: setattr(f, "process_version", 2),
                   lambda f: f.assignment.update(process_state="uncertain"),
                   lambda f: f.log.append(dict(kind="prompt", launch_id=3)),
                   lambda f: f.snap.update(generation=99), lambda f: (f.folder / "receipt.json").unlink()]
        for i, change in enumerate(changes):
            with self.subTest(i=i):
                root = self.root / str(i)
                root.mkdir()
                f = Fake(root)
                self.completed_attempt(f)
                change(f)
                with patch.object(f, "report") as report:
                    with self.assertRaises(RuntimeError):
                        f.run_test()
                    report.assert_not_called()
                self.assertEqual(f.sent_count, 0)

    def test_uncertain_assignment_refuses_before_initialization(self):
        self.fake.assignment["process_state"] = "uncertain"
        with self.assertRaisesRegex(RuntimeError, "reservation changed"):
            self.fake.run_test()
        self.assertEqual(self.fake.sent_count, 0)
        self.assertFalse((self.fake.folder / "init.json").exists())
        self.assertEqual(len(self.fake.calls), 1)  # Only the fake campaign read.

    def test_query_subprocess_clears_inherited_worker_identity(self):
        args = [self.fake.taskr, "--json", "work", "list", "1"]
        with patch.dict(os.environ, TASKR_TASK="998", TASKR_LAUNCH="997", TASKR_DB=self.fake.context["db"]):
            with patch.object(m.subprocess, "Popen", side_effect=RuntimeError("captured without process")) as popen:
                with self.assertRaisesRegex(RuntimeError, "captured without process"):
                    m.Adapter.command(self.fake, args)
            popen.assert_called_once()
            self.assertEqual(popen.call_args.args[0], args)
            env = popen.call_args.kwargs["env"]
            self.assertNotIn("TASKR_TASK", env)
            self.assertNotIn("TASKR_LAUNCH", env)
            self.assertEqual(env["TASKR_DB"], self.fake.context["db"])
            self.assertEqual(os.environ["TASKR_TASK"], "998")
            self.assertEqual(os.environ["TASKR_LAUNCH"], "997")

    def test_macos_bsdinfo_failure_clauses_are_pid_specific_and_bounded(self):
        uid = os.geteuid()
        cases = [(0, errno.ESRCH, 0, 0, 0, "gone"), (0, errno.EPERM, 0, 0, 0, "denied"),
                 (0, errno.EACCES, 0, 0, 0, "denied"), (0, 0, 0, 0, 0, "empty-result"),
                 (64, 0, 0, 0, 0, "size-or-query-failure"), (136, 0, 31, uid, 2, "pid-mismatch"),
                 (136, 0, 30, uid + 1, 2, "foreign-uid"), (136, 0, 30, uid, 5, "zombie")]
        for returned, error, reported_pid, reported_uid, status, reason in cases:
            with self.subTest(reason=reason, error=error):
                lib = Mock()
                def pidinfo(pid, flavor, arg, output, size):
                    self.assertEqual(size, 136)
                    info = output._obj
                    info.pid, info.uid, info.status = reported_pid, reported_uid, status
                    m.ctypes.set_errno(error)
                    return returned
                lib.proc_pidinfo.side_effect = pidinfo
                with patch.object(m.sys, "platform", "darwin"), patch.object(m.ctypes, "CDLL", return_value=lib):
                    with self.assertRaisesRegex(RuntimeError, f"pid=30 op=proc_pidinfo class={reason}") as caught:
                        m.Adapter.process(self.fake, 30)
                text = str(caught.exception)
                self.assertIn(f"bytes={returned}/136 errno={error}", text)
                self.assertIn(f"returned_pid={reported_pid} uid={reported_uid}/{uid} status={status}", text)
                self.assertLess(len(text), 300)
                lib.proc_pidpath.assert_not_called()
        # A successful library call need not overwrite errno: clear stale errno before probing.
        lib = Mock()
        lib.proc_pidinfo.return_value = 0
        m.ctypes.set_errno(errno.EPERM)
        with patch.object(m.sys, "platform", "darwin"), patch.object(m.ctypes, "CDLL", return_value=lib):
            with self.assertRaisesRegex(RuntimeError, "class=empty-result.*errno=0"):
                m.Adapter.process(self.fake, 30)

    def test_macos_executable_probe_failure_identifies_pid_and_errno(self):
        lib = Mock()
        def pidinfo(pid, flavor, arg, output, size):
            info = output._obj
            info.pid, info.uid, info.status = pid, os.geteuid(), 2
            return size
        def pidpath(*args):
            m.ctypes.set_errno(errno.ESRCH)
            return 0
        lib.proc_pidinfo.side_effect = pidinfo
        lib.proc_pidpath.side_effect = pidpath
        with patch.object(m.sys, "platform", "darwin"), patch.object(m.ctypes, "CDLL", return_value=lib):
            with self.assertRaisesRegex(RuntimeError, f"pid=30 op=proc_pidpath bytes=0 errno={errno.ESRCH}"):
                m.Adapter.process(self.fake, 30)

    def test_disappeared_foreground_rows_are_never_silently_skipped(self):
        self.fake.guard()
        for failing_pid, hint in ((31, "node"), (31, "codex"), (30, "codex")):
            with self.subTest(pid=failing_pid, hint=hint):
                original = self.fake.herdr
                def herdr(*args):
                    value = original(*args)
                    if args[:2] == ("pane", "process-info"):
                        value["process_info"]["foreground_processes"] = [dict(pid=30, name="codex"), dict(pid=31, name=hint)]
                    return value
                original_process = self.fake.process
                def process(pid):
                    if pid == failing_pid:
                        raise RuntimeError(f"native process identity unavailable: pid={pid} op=proc_pidinfo class=gone bytes=0/136 errno=3")
                    return original_process(pid)
                with patch.object(self.fake, "herdr", side_effect=herdr), patch.object(self.fake, "process", side_effect=process):
                    with self.assertRaisesRegex(RuntimeError, f'foreground pid={failing_pid} name_hint="{hint}".*class=gone') as caught:
                        self.fake.run_test()
                    self.assertIn("pinned_native_pid=30", str(caught.exception))
                self.assertEqual(self.fake.sent_count, 0)
                self.assertFalse((self.fake.folder / "init.json").exists())

    def test_private_attempt_directory_lock_and_outside_checkout(self):
        self.fake.native = ID
        with patch.dict(os.environ, HOME=str(self.root / "home")):
            result = self.fake.run()
            folder = Path(result["provenance_path"]).parent
            self.assertEqual(folder.stat().st_mode & 0o777, 0o700)
            self.assertNotIn(self.fake.cwd, folder.parents)
            fd = os.open(folder / "lock", os.O_RDWR)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                with self.assertRaises(BlockingIOError):
                    self.fake.run()
            finally:
                os.close(fd)
        with patch.dict(os.environ, HOME=str(self.fake.cwd)):
            with self.assertRaisesRegex(RuntimeError, "outside checkout"):
                self.fake.run()

    def test_unknown_delivery_never_repeats_even_if_metadata_appears(self):
        self.fake.fault = "unknown-delivery"
        with self.assertRaisesRegex(OSError, "unknown delivery"):
            self.fake.run_test()
        self.fake.fault = ""
        self.fake.native = ID
        with self.assertRaisesRegex(RuntimeError, "never resend"):
            self.fake.run_test()
        self.assertEqual(self.fake.sent_count, 1)

    def test_reconfigured_attempt_refuses_instead_of_resending(self):
        self.fake.fault = "unknown-delivery"
        with self.assertRaises(OSError):
            self.fake.run_test()
        self.fake.fault = ""
        self.fake.generation = 5
        self.fake.context["generation"] = 5
        self.fake.snap["generation"] = 5
        with self.assertRaisesRegex(RuntimeError, "stale bootstrap"):
            self.fake.run_test()
        self.assertEqual(self.fake.sent_count, 1)

    def test_pre_prompt_guards(self):
        changes = [lambda f: f.snap.update(generation=99), lambda f: f.snap.update(parked=True),
                   lambda f: f.assignment.update(state="running"), lambda f: f.assignment.update(task_id=9),
                   lambda f: f.assignment["lane"].update(provider="claude"),
                   lambda f: f.snap["work"][0].update(current_run=9),
                   lambda f: f.task_state.update(current_launch_id=8),
                   lambda f: f.log.append(dict(kind="prompt", launch_id=3)),
                   lambda f: setattr(f, "fault", "duplicate")]
        for i, change in enumerate(changes):
            with self.subTest(i=i):
                root = self.root / str(i)
                root.mkdir()
                f = Fake(root)
                change(f)
                with self.assertRaises(RuntimeError):
                    f.run_test()
                self.assertEqual(f.sent_count, 0)

    def test_identity_and_generation_races_withhold_reporting(self):
        for fault in ("replace-process", "generation-race", "wrong-env"):
            with self.subTest(fault=fault):
                root = self.root / fault
                root.mkdir()
                f = Fake(root, fault)
                with self.assertRaises(RuntimeError):
                    f.run_test()
                self.assertIsNone(f.native)

    def test_conflicting_native_metadata(self):
        self.fake.native = ID
        self.fake.log[0]["session_ref"] = OTHER
        with self.assertRaisesRegex(RuntimeError, "conflicting"):
            self.fake.run_test()

    def test_completed_receipt_rejects_tampering_and_partial_data(self):
        thread = self.report_server()
        self.fake.run_test()
        thread.join(3)
        receipt = self.fake.folder / "receipt.json"
        original = receipt.read_bytes()
        value = json.loads(original)
        for field, bad in (("nonce", "stale"), ("CODEX_SESSION_ID", OTHER), ("TASKR_TASK", "99"),
                           ("extra", "not-allowlisted")):
            with self.subTest(field=field):
                changed = dict(value)
                changed[field] = bad
                receipt.write_bytes(m.encoded(changed))
                with self.assertRaises(RuntimeError):
                    self.fake.run_test()
                self.assertEqual(self.fake.sent_count, 1)
        receipt.write_bytes(b'{"version":')
        with self.assertRaises(ValueError):
            self.fake.run_test()
        receipt.write_bytes(original)
        receipt.chmod(0o644)
        with self.assertRaisesRegex(RuntimeError, "unsafe"):
            self.fake.run_test()
        receipt.chmod(0o600)
        receipt.unlink()
        receipt.symlink_to(self.root / "missing")
        with self.assertRaisesRegex(RuntimeError, "never resend"):
            self.fake.run_test()

    def test_executable_symlink_and_relative_scratch_context(self):
        alias = self.root / "alias"
        alias.mkdir()
        (alias / "taskr").symlink_to(self.root / "taskr")
        with patch.dict(os.environ, TASKR_WORK_EXECUTABLE=str(alias / "taskr"), TASKR_DB="scratch.db"):
            f = m.Adapter(["2", "3", "1", "4", "impl-w3", "w1:p1", str(self.fake.cwd)])
            self.assertEqual(f.taskr, str(alias / "taskr"))
            self.assertEqual(os.environ["TASKR_DB"], str(Path("scratch.db").resolve()))

    def test_cli_preflight_output_and_time_bounds(self):
        f = self.fake
        f.command = m.Adapter.command.__get__(f)
        f.fault = "server-down"
        marker = self.root / "not-started"
        with self.assertRaises(OSError):
            f.command([sys.executable, "-c", f"open({str(marker)!r}, 'w').close()"])
        self.assertFalse(marker.exists())
        f.fault = ""
        with self.assertRaisesRegex(RuntimeError, "output limit"):
            f.command([sys.executable, "-c", "import os; os.write(1, b'x' * 1100000)"])
        f.deadline = time.monotonic() + 0.1
        with self.assertRaisesRegex(RuntimeError, "deadline"):
            f.command([sys.executable, "-c", "import signal; signal.pause()"])

    def test_artifact_symlinks_permissions_and_partial_receipt(self):
        self.fake.native = ID
        self.fake.run_test()
        path = self.fake.folder / "provenance.json"
        data = path.read_bytes()
        path.unlink()
        target = self.root / "target"
        target.write_bytes(data)
        path.symlink_to(target)
        with self.assertRaises(OSError):
            self.fake.run_test()
        path.unlink()
        m.write_once(path, data)
        path.chmod(0o644)
        with self.assertRaisesRegex(RuntimeError, "unsafe"):
            self.fake.run_test()

    def test_native_id_format_and_same_process_snapshot(self):
        for value in ("", "latest", ID.upper(), None):
            if value == ID:
                continue
            with self.assertRaises((RuntimeError, ValueError, AttributeError)):
                m.native_id(value)
        if sys.platform in ("darwin", "linux"):
            self.fake.command = m.Adapter.command.__get__(self.fake)
            process = m.Adapter.process(self.fake, os.getpid())
            self.assertEqual(process["pid"], os.getpid())
            self.assertEqual(process["group"], os.getpgid(os.getpid()))
            self.assertTrue(process["start"])


if __name__ == "__main__":
    unittest.main()
