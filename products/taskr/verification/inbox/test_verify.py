"""Driver self-checks; no Java, taskr, builds, or second state exploration engine."""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import verify


class DriverChecks(unittest.TestCase):
    def test_executable_symlink_preserves_multicall_name_and_hashes_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "rustup"
            target.write_text("#!/bin/sh\ncase \"$0:$1:$2\" in\n"
                              "*/cargo:build:--offline) echo cargo-dispatched ;;\n"
                              "*) exit 64 ;;\nesac\n")
            target.chmod(0o755)
            shim = root / "cargo"
            shim.symlink_to("rustup")
            requested = verify.executable(str(shim), "cargo")
            self.assertEqual(requested, shim.absolute())
            self.assertEqual(verify.sha(requested), verify.sha(target))
            run = verify.Run(root, 1)
            code, output = run.command([requested, "build", "--offline"], root)
            self.assertEqual((code, output), (0, "cargo-dispatched\n"))
            target.chmod(0o644)
            with self.assertRaises(verify.Blocked):
                verify.executable(str(shim), "cargo")
            target.unlink()
            with self.assertRaises(verify.Blocked):
                verify.executable(str(shim), "cargo")

    def test_keyboard_interrupt_cleans_up_and_propagates_through_case(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run = verify.Run(root, 1)
            interruption = KeyboardInterrupt()
            child = Mock(pid=12345)
            child.communicate.side_effect = [interruption, ("partial output", "stderr")]
            with patch.object(verify.subprocess, "Popen", return_value=child), \
                    patch.object(verify.os, "killpg") as kill:
                with self.assertRaises(KeyboardInterrupt) as caught:
                    run.case("MODEL-NC1", lambda: run.command(["fixture"], root))
            self.assertIs(caught.exception, interruption)
            kill.assert_called_once_with(child.pid, verify.signal.SIGKILL)
            self.assertEqual(child.communicate.call_count, 2)
            self.assertEqual(len(run.commands), 1)
            self.assertIsNone(run.commands[0]["exit"])
            self.assertEqual((root / "command-001.log").read_text(), "partial outputstderr")
            self.assertEqual(run.results["MODEL-NC1"]["outcome"], "blocked")

    def test_source_drift_is_blocked_without_rewriting_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "covered.rs").write_text("before")
            manifest = {"source_commit": "synthetic", "source_files": {
                "covered.rs": verify.sha(root / "covered.rs")}}
            before = json.dumps(manifest)
            verify.check_sync(manifest, root)
            (root / "covered.rs").write_text("after")
            with self.assertRaises(verify.Blocked):
                verify.check_sync(manifest, root)
            self.assertEqual(json.dumps(manifest), before)

    def check_tlc_result(self, name, code, output, mutation="baseline"):
        with tempfile.TemporaryDirectory() as directory:
            run = verify.Run(Path(directory), 1)
            run.commands.append({"log": "synthetic.log"})
            args = SimpleNamespace(java="java", tlc_jar="tool.jar")
            with patch.object(run, "command", return_value=(code, output)):
                return verify.tlc(run, args, name, mutation=mutation)

    def test_negative_control_requires_exact_violation_and_trace(self):
        output = ("Error: Invariant AckAuthority is violated.\nState 1: fixture\n"
                  "State 2: fixture\n10 states generated, 7 distinct states found, "
                  "3 states left on queue.\n")
        self.assertEqual(self.check_tlc_result("nc", 12, output, "NC1")
                         ["counterexample_invariant"], "AckAuthority")
        for code, bad in [(150, output), (12, output.replace("AckAuthority", "TypeOK")),
                          (12, output.replace("State 2:", "no trace:")),
                          (None, output), (12, "parse error")]:
            with self.subTest(code=code, output=bad):
                with self.assertRaises((AssertionError, verify.Blocked)):
                    self.check_tlc_result("nc", code, bad, "NC1")

    def test_baseline_requires_finished_nonvacuous_exploration(self):
        success = ("Model checking completed. No error has been found.\n"
                   "10 states generated, 7 distinct states found, 0 states left on queue.\n")
        self.check_tlc_result("base", 0, success)
        for output in (success.replace("7 distinct", "1 distinct"),
                       success.replace("0 states left", "2 states left"),
                       success.replace("Model checking completed. No error has been found.", "")):
            with self.assertRaises(AssertionError):
                self.check_tlc_result("base", 0, output)

    def test_missing_cases_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            run = verify.Run(Path(directory), 1)
            run.case("SYNC", lambda: {})
            result = json.loads((Path(directory) / "result.json").read_text())
            self.assertEqual(result["outcome"], "blocked")
            self.assertEqual(set(result["cases"]), set(verify.CASES))


if __name__ == "__main__":
    unittest.main()
