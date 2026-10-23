"""Driver self-checks; no Java, taskr, builds, or second state exploration engine."""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import verify


class DriverChecks(unittest.TestCase):
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
