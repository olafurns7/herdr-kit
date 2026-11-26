#!/usr/bin/env python3
"""herdr0 against a fake hc (tests/fake_hc): no real hc, core or herdr runs here.

Run: python3 -B products/setup/tests/herdr0_test.py
"""
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
HERDR0 = os.path.join(HERE, "..", "bin", "herdr0")
FAKE = os.path.join(HERE, "fake_hc")
ROOT_LINE = ("hc run --bg --name NAME --keep --no-parent --cwd DIR -- "
             "herdr0 enter --new-workspace LABEL -- CMD")


def serve(core):
    """Accept and drop connections, as a live core would answer herdr0's socket check."""
    def loop():
        while True:
            try:
                conn, _ = core.accept()
            except OSError:
                return
            conn.close()
    threading.Thread(target=loop, daemon=True).start()


class Herdr0(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="herdr0-")
        self.state = os.path.join(self.tmp, "fake")
        os.mkdir(self.state)
        self.sock_path = os.path.join(self.tmp, "core.sock")
        self.core = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.core.bind(self.sock_path)
        self.core.listen(64)
        serve(self.core)
        self.env = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "HOME": self.tmp,
            "XDG_STATE_HOME": os.path.join(self.tmp, "state"),
            "XDG_CONFIG_HOME": os.path.join(self.tmp, "config"),
            "HC_SOCKET": self.sock_path,
            "HC_BIN": FAKE,
            "FAKE_HC_STATE": self.state,
            "SHELL": "/bin/sh",
        }

    def tearDown(self):
        # Contract 4: every hc run herdr0 issued in this test passed --no-parent.
        for argv in self.calls():
            if argv[0] == "run":
                self.assertIn("--no-parent", argv[: argv.index("--")], argv)
        self.core.close()
        shutil.rmtree(self.tmp)

    # helpers
    def herdr(self, *args, env=None, ok=True):
        e = dict(self.env, **(env or {}))
        p = subprocess.run([sys.executable, "-B", HERDR0, *args], capture_output=True, text=True,
                           env=e, timeout=60)
        if ok:
            self.assertEqual(p.returncode, 0, f"herdr {args}: {p.stderr}")
        return p

    def js(self, *args, **kw):
        return json.loads(self.herdr(*args, **kw).stdout)["result"]

    def calls(self):
        try:
            with open(os.path.join(self.state, "calls.jsonl")) as f:
                return [json.loads(line) for line in f]
        except FileNotFoundError:
            return []

    def clear_calls(self):
        open(os.path.join(self.state, "calls.jsonl"), "w").close()

    def fake(self):
        with open(os.path.join(self.state, "state.json")) as f:
            return json.load(f)

    def put(self, s):
        with open(os.path.join(self.state, "state.json"), "w") as f:
            json.dump(s, f)

    def edit(self, addr, **fields):
        s = self.fake()
        s["terminals"][addr].update(fields)
        self.put(s)

    def workspace(self, label="demo"):
        return self.js("workspace", "create", "--label", label, "--cwd", self.tmp, "--no-focus")

    def error(self, p):
        return json.loads(p.stderr)["error"]["code"]

    # ids, layout
    def test_workspace_and_tab_create_keep_herdr_id_shapes(self):
        r = self.workspace()
        ws = r["workspace"]["workspace_id"]
        self.assertRegex(ws, r"^w[0-9A-Z]{4}$")
        self.assertEqual(r["tab"]["tab_id"], f"{ws}:t1")
        self.assertEqual(r["root_pane"]["pane_id"], f"{ws}:p1")
        self.assertEqual(r["root_pane"]["workspace_id"], ws)
        self.assertEqual(r["workspace"]["label"], "demo")
        r2 = self.js("tab", "create", "--workspace", ws, "--cwd", self.tmp, "--label", "impl-x",
                     "--env", "FLEET_ROLE=worker", "--env", "DISABLE_AUTO_UPDATE=true", "--no-focus")
        self.assertEqual(r2["tab"]["tab_id"], f"{ws}:t2")
        self.assertEqual(r2["root_pane"]["pane_id"], f"{ws}:p2")
        self.assertEqual(r2["tab"]["label"], "impl-x")
        run = [c for c in self.calls() if c[0] == "run"][-1]
        argv = run[run.index("--") + 1:]
        self.assertEqual(argv[:3], ["env", "FLEET_ROLE=worker", "DISABLE_AUTO_UPDATE=true"])
        self.assertEqual(argv[3:], [os.path.realpath(HERDR0), "enter", "--workspace", ws, "--tab", f"{ws}:t2",
                                    "--pane", f"{ws}:p2", "--"])
        for kv in (f"ws={ws}", f"tab={ws}:t2", f"pane={ws}:p2", "tab_label=impl-x", "ws_label=demo"):
            self.assertIn(kv, run)
        self.assertIn("--cwd", run)
        # herdr-launch's json_str reads tab_id and pane_id from the whole reply: one value each.
        out = self.herdr("tab", "create", "--workspace", ws, "--label", "y").stdout
        self.assertEqual(set(re.findall(r'"tab_id":"([^"]*)"', out)), {f"{ws}:t3"})
        self.assertEqual(set(re.findall(r'"pane_id":"([^"]*)"', out)), {f"{ws}:p3"})

    def test_tab_create_in_unknown_workspace_is_not_found(self):
        p = self.herdr("tab", "create", "--workspace", "wNONE", ok=False)
        self.assertEqual(p.returncode, 1)
        self.assertEqual(self.error(p), "workspace_not_found")
        self.assertFalse([c for c in self.calls() if c[0] == "run"])

    def test_tab_create_in_existing_workspace_gets_new_numbers_and_keeps_ws_label(self):
        ws = self.workspace("demo")["workspace"]["workspace_id"]
        # The fake's ls prints short rows (no labels), as the real hc CLI does.
        p = subprocess.run([FAKE, "ls", "--json"], capture_output=True, text=True, env=self.env, check=True)
        self.assertNotIn("labels", json.loads(p.stdout)["terminals"][0])
        r = self.js("tab", "create", "--workspace", ws, "--label", "two")
        self.assertEqual((r["tab"]["tab_id"], r["root_pane"]["pane_id"]), (f"{ws}:t2", f"{ws}:p2"))
        self.assertEqual(self.fake()["terminals"]["t2"]["labels"]["ws_label"], "demo")
        r = self.js("tab", "create", "--workspace", ws)
        self.assertEqual((r["tab"]["tab_id"], r["root_pane"]["pane_id"]), (f"{ws}:t3", f"{ws}:p3"))
        self.assertEqual(self.fake()["terminals"]["t3"]["labels"]["ws_label"], "demo")
        self.assertEqual(self.js("pane", "get", f"{ws}:p1")["pane"]["pane_id"], f"{ws}:p1")
        self.assertEqual(len(self.js("pane", "list", "--workspace", ws)["panes"]), 3)
        self.assertEqual(self.js("workspace", "list")["workspaces"][0]["label"], "demo")

    def test_lists_and_gets(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.js("tab", "create", "--workspace", ws, "--label", "two")
        tabs = self.js("tab", "list", "--workspace", ws)["tabs"]
        self.assertEqual([t["tab_id"] for t in tabs], [f"{ws}:t1", f"{ws}:t2"])
        self.assertEqual([t["pane_count"] for t in tabs], [1, 1])
        panes = self.js("pane", "list", "--workspace", ws)["panes"]
        self.assertEqual([p["pane_id"] for p in panes], [f"{ws}:p1", f"{ws}:p2"])
        pane = self.js("pane", "get", f"{ws}:p2")["pane"]
        self.assertEqual((pane["tab_id"], pane["workspace_id"], pane["cwd"]), (f"{ws}:t2", ws, "/srv/work"))
        self.assertNotIn("agent", pane)
        self.assertEqual(self.js("tab", "get", f"{ws}:t2")["tab"]["label"], "two")
        self.herdr("tab", "rename", f"{ws}:t2", "renamed")
        self.assertIn(["label", "--json", "t2", "tab_label=renamed"], self.calls())
        self.assertEqual(self.js("tab", "get", f"{ws}:t2")["tab"]["label"], "renamed")
        wss = self.js("workspace", "list")["workspaces"]
        self.assertEqual([(w["workspace_id"], w["label"], w["tab_count"]) for w in wss], [(ws, "demo", 2)])
        p = self.herdr("pane", "get", f"{ws}:p9", ok=False)
        self.assertEqual((p.returncode, self.error(p)), (1, "pane_not_found"))

    def test_pane_current_uses_hc_terminal_then_herdr_pane(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.js("tab", "create", "--workspace", ws)
        cur = self.js("pane", "current", "--current", env={"HC_TERMINAL_ID": "t2", "HERDR_PANE_ID": f"{ws}:p1"})
        self.assertEqual(cur["pane"]["pane_id"], f"{ws}:p2")
        cur = self.js("pane", "current", env={"HERDR_PANE_ID": f"{ws}:p1"})
        self.assertEqual(cur["pane"]["pane_id"], f"{ws}:p1")

    def test_tab_and_workspace_close_kill_and_remove_by_label(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.js("tab", "create", "--workspace", ws)
        self.js("tab", "create", "--workspace", ws)
        self.clear_calls()
        self.herdr("tab", "close", f"{ws}:t2")
        self.assertEqual([c for c in self.calls() if c[0] in ("kill", "rm")],
                         [["kill", "--json", "t2"], ["rm", "--json", "t2"]])
        self.herdr("workspace", "close", ws)
        self.assertEqual(self.fake()["terminals"], {})
        p = self.herdr("tab", "close", f"{ws}:t1", ok=False)
        self.assertEqual(self.error(p), "tab_not_found")

    # status
    def test_status_map(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        for _ in range(6):
            self.js("tab", "create", "--workspace", ws)
        words = {"t1": "idle", "t2": "working", "t3": "blocked", "t4": "done", "t5": "error", "t6": "thinking"}
        for addr, word in words.items():
            self.edit(addr, status={"state": word}, reported=True)
        self.edit("t7", status=None, labels=dict(self.fake()["terminals"]["t7"]["labels"], agent="claude",
                                                 agent_name="impl-a"),
                  foreground={"pid": 7, "name": "claude", "argv": ["claude"], "cwd": "/x"})
        agents = {a["pane_id"]: a for a in self.js("agent", "list")["agents"]}
        got = {pid.split(":")[1]: a["agent_status"] for pid, a in agents.items()}
        self.assertEqual(got, {"p1": "idle", "p2": "working", "p3": "blocked", "p4": "done", "p5": "blocked",
                               "p6": "unknown", "p7": "unknown"})
        self.assertEqual(agents[f"{ws}:p7"]["name"], "impl-a")
        self.assertEqual(agents[f"{ws}:p7"]["agent"], "claude")
        # A shell back at its prompt with no report is no agent.
        self.edit("t7", foreground={"pid": 7, "name": "-zsh", "argv": ["zsh"], "cwd": "/x"})
        self.assertNotIn(f"{ws}:p7", [a["pane_id"] for a in self.js("agent", "list")["agents"]])

    # agents
    def test_agent_start_prompt_wait_get(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        s = self.fake()
        s["on_send"] = [{"match": "claude --account", "status": "idle", "foreground": "claude"},
                        {"match": "do the thing", "status": "working", "turn": 0},
                        {"match": "finish", "status": "idle", "turn": 1}]
        self.put(s)
        # herdr-launch's shape: the words after -- are arguments to the kind's program.
        r = self.js("agent", "start", "impl-a", "--kind", "claude", "--pane", f"{ws}:p1", "--timeout", "5000",
                    "--", "--account", "acct1", "--model", "m 1")
        self.assertEqual((r["agent"]["name"], r["agent"]["agent"], r["agent"]["agent_status"]),
                         ("impl-a", "claude", "idle"))
        self.assertIn(["send", "--json", "t1", "--", "claude --account acct1 --model 'm 1'"], self.calls())
        self.assertIn(["label", "--json", "t1", "agent=claude", "agent_name=impl-a", "agent_session="],
                      self.calls())
        p = self.herdr("agent", "start", "impl-a", "--kind", "claude", "--pane", f"{ws}:p1", "--", "x", ok=False)
        self.assertEqual(self.error(p), "agent_name_taken")
        r = self.js("agent", "prompt", "impl-a", "do the thing", "--wait", "--until", "working", "--until",
                    "blocked", "--timeout", "20000")
        self.assertEqual(r["agent"]["agent_status"], "working")
        # The turn already ended: a later turn counts as the work having started.
        r = self.js("agent", "prompt", f"{ws}:p1", "finish", "--wait", "--until", "working", "--timeout", "3000")
        self.assertEqual(r["agent"]["agent_status"], "idle")
        self.assertEqual(self.js("agent", "wait", "impl-a", "--until", "idle", "--timeout", "2000")
                         ["agent"]["agent_status"], "idle")
        self.assertEqual(self.js("agent", "get", "impl-a")["agent"]["pane_id"], f"{ws}:p1")
        p = self.herdr("agent", "wait", "impl-a", "--until", "working", "--timeout", "1000", ok=False)
        self.assertEqual((p.returncode, self.error(p)), (1, "timeout"))
        self.clear_calls()
        self.herdr("agent", "prompt", "impl-a", "plain", "--wait", "--timeout", "5000")
        self.assertIn(["send", "--json", "--wait", "--timeout", "5", "t1", "--", "plain"], self.calls())

    def test_agent_prompt_refuses_blocked(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.edit("t1", status={"state": "blocked"}, reported=True)
        self.clear_calls()
        p = self.herdr("agent", "prompt", f"{ws}:p1", "hello", "--wait", "--until", "working", ok=False)
        self.assertEqual((p.returncode, self.error(p)), (1, "agent_blocked"))
        self.assertFalse([c for c in self.calls() if c[0] == "send"])

    def test_agent_start_at_dialog_fails_blocked(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        s = self.fake()
        s["on_send"] = [{"match": "codex", "status": "blocked", "foreground": "codex"}]
        self.put(s)
        p = self.herdr("agent", "start", "impl-c", "--kind", "codex", "--pane", f"{ws}:p1", "--", ok=False)
        self.assertEqual(self.error(p), "agent_blocked")
        # No words after --: the bare kind starts.
        self.assertIn(["send", "--json", "t1", "--", "codex"], self.calls())

    def test_agent_start_without_kind_types_the_argv(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        s = self.fake()
        s["on_send"] = [{"match": "/opt/tool", "status": "idle", "foreground": "tool"}]
        self.put(s)
        r = self.js("agent", "start", "impl-t", "--pane", f"{ws}:p1", "--timeout", "2000", "--",
                    "/opt/tool", "--flag", "a b")
        self.assertEqual(r["agent"]["agent"], "tool")
        self.assertIn(["send", "--json", "t1", "--", "/opt/tool --flag 'a b'"], self.calls())
        p = self.herdr("agent", "start", "impl-u", "--pane", f"{ws}:p1", "--", ok=False)
        self.assertEqual(p.returncode, 2)

    # panes
    def test_pane_read_run_keys_metadata_process_info(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.edit("t1", screen="a\nb\nc\nd\n\n\n")
        self.assertEqual(self.herdr("pane", "read", f"{ws}:p1", "--source", "visible", "--lines", "2").stdout,
                         "c\nd\n")
        self.assertIn(["read", "--json", "t1", "--lines", "0"], self.calls())
        self.herdr("pane", "read", f"{ws}:p1", "--source", "recent-unwrapped", "--lines", "40")
        self.assertIn(["read", "--json", "t1", "--lines", "40"], self.calls())
        self.assertEqual(self.herdr("agent", "read", f"{ws}:p1", "--source", "visible").stdout, "a\nb\nc\nd\n")
        self.herdr("pane", "run", f"{ws}:p1", " export A=1")
        self.assertIn(["send", "--json", "t1", "--", " export A=1"], self.calls())
        self.herdr("pane", "send-keys", f"{ws}:p1", "Enter", "esc", "ctrl+c", "y")
        self.assertIn(["keys", "--json", "t1", "enter", "esc", "ctrl+c", "y"], self.calls())
        self.herdr("pane", "report-metadata", f"{ws}:p1", "--source", "taskr", "--token", "taskr_state=ready",
                   "--token", "taskr_round=2")
        self.assertIn(["label", "--json", "t1", "tok_taskr_state=ready", "tok_taskr_round=2"], self.calls())
        info = self.js("pane", "process-info", "--pane", f"{ws}:p1")["process_info"]
        self.assertEqual(info["shell_pid"], info["foreground_process_group_id"])
        self.assertEqual(info["foreground_processes"][0]["name"], "zsh")
        self.edit("t1", foreground={"pid": 4242, "name": "claude", "argv": ["claude", "-x"], "cwd": "/w"})
        info = self.js("pane", "process-info", "--pane", f"{ws}:p1")["process_info"]
        self.assertNotEqual(info["shell_pid"], info["foreground_process_group_id"])
        self.assertEqual(info["foreground_processes"][0]["cmdline"], "claude -x")

    def test_send_keys_takes_herdr_09_and_tmux_key_names(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.clear_calls()
        self.herdr("pane", "send-keys", f"{ws}:p1", "C-c", "Enter", "Escape", "Tab", "escape", "M-x", "C-M-x",
                   "F5", "PageUp", "BSpace", "Ctrl+C", "Y")
        self.assertEqual([c for c in self.calls() if c[0] == "keys"],
                         [["keys", "--json", "t1", "ctrl+c", "enter", "esc", "tab", "esc", "alt+x", "ctrl+alt+x",
                           "f5", "pgup", "backspace", "ctrl+c", "Y"]])
        # Unknown names exit 2 before any key is sent, as herdr validated every key before writing.
        for bad in ("bogus", "C-", "ctrl+nope", "x+y"):
            self.clear_calls()
            p = self.herdr("pane", "send-keys", f"{ws}:p1", "Enter", bad, ok=False)
            self.assertEqual((p.returncode, self.error(p)), (2, "unknown_key"), bad)
            self.assertFalse([c for c in self.calls() if c[0] == "keys"], bad)

    def test_send_text_types_the_text_without_enter(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.clear_calls()
        r = self.js("pane", "send-text", f"{ws}:p1", "ls -A\tx")
        self.assertEqual(r, {"pane_id": f"{ws}:p1", "type": "pane_text"})
        self.assertEqual([c for c in self.calls() if c[0] in ("keys", "send")],
                         [["keys", "--json", "t1", "l", "s", "space", "-", "A", "tab", "x"]])
        self.clear_calls()
        self.herdr("pane", "send-text", f"{ws}:p1", "")
        self.assertFalse([c for c in self.calls() if c[0] in ("keys", "send")])
        p = self.herdr("pane", "send-text", f"{ws}:p1", "a", "b", ok=False)
        self.assertEqual((p.returncode, self.error(p)), (2, "usage"))
        p = self.herdr("pane", "send-text", f"{ws}:p1", "a\x1c", ok=False)
        self.assertEqual(p.returncode, 2)

    def test_wait_output_checks_the_screen_first(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.edit("t1", screen="herdr-launch-env=1/2/worker/2\n")
        self.clear_calls()
        self.herdr("pane", "wait-output", f"{ws}:p1", "--match", "herdr-launch-env=1/2/worker/2",
                   "--timeout", "15000")
        self.assertFalse([c for c in self.calls() if c[0] == "wait"])

    def test_wait_output_waits_for_later_output_and_times_out(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        s = self.fake()
        s["future"] = {"t1": ["probe-end\n"]}
        self.put(s)
        self.herdr("pane", "wait-output", f"{ws}:p1", "--match", "probe-end", "--timeout", "5000")
        self.assertIn(["wait", "--json", "--output=probe-end", "--timeout", "2", "t1"], self.calls())
        p = self.herdr("pane", "wait-output", f"{ws}:p1", "--match", "never", "--timeout", "1000", ok=False)
        self.assertEqual((p.returncode, self.error(p)), (1, "timeout"))

    def test_agent_session_from_the_agents_own_args(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        for _ in range(3):
            self.js("tab", "create", "--workspace", ws)
        s = self.fake()
        s["on_send"] = [{"match": "claude", "status": "idle", "foreground": "claude"},
                        {"match": "codex", "status": "idle", "foreground": "codex"}]
        self.put(s)
        cases = [("impl-a", "claude", ["--account", "acct1", "--resume", "S-1", "--model", "m"], "S-1"),
                 ("impl-b", "claude", ["--session-id=S-2"], "S-2"),
                 ("impl-c", "codex", ["--account", "acct1", "resume", "S-3", "--yolo"], "S-3"),
                 ("impl-d", "claude", ["-r", "S-4"], "S-4")]
        for n, (name, kind, args, sid) in enumerate(cases, 1):
            r = self.js("agent", "start", name, "--kind", kind, "--pane", f"{ws}:p{n}", "--timeout", "2000",
                        "--", *args)
            self.assertEqual(r["agent"]["agent_session"]["value"], sid, name)
            self.assertIn(["label", "--json", f"t{n}", f"agent={kind}", f"agent_name={name}",
                           f"agent_session={sid}"], self.calls())
        self.assertEqual(self.js("agent", "get", "impl-c")["agent"]["agent_session"]["value"], "S-3")
        # A fresh start names no session: the label stays empty and herdr0 shows none.
        self.js("tab", "create", "--workspace", ws)
        r = self.js("agent", "start", "impl-e", "--kind", "codex", "--pane", f"{ws}:p5", "--timeout", "2000",
                    "--", "resume", "--last")
        self.assertNotIn("agent_session", r["agent"])
        self.assertIn(["label", "--json", "t5", "agent=codex", "agent_name=impl-e", "agent_session="],
                      self.calls())

    def test_send_and_keys_run_without_the_callers_terminal_identity(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        open(os.path.join(self.state, "env.jsonl"), "w").close()
        env = {"HC_TERMINAL_ID": "t9", "HC_TERMINAL_SOCKET": "/x/holder.sock"}
        self.herdr("pane", "run", f"{ws}:p1", "true", env=env)
        self.herdr("pane", "send-keys", f"{ws}:p1", "Enter", env=env)
        self.herdr("pane", "send-text", f"{ws}:p1", "y", env=env)
        self.herdr("agent", "prompt", f"{ws}:p1", "hello", env=env)
        with open(os.path.join(self.state, "env.jsonl")) as f:
            seen = [json.loads(line) for line in f]
        for row in seen:
            if row["cmd"] in ("send", "keys"):
                self.assertIsNone(row["HC_TERMINAL_ID"], row)
                self.assertIsNone(row["HC_TERMINAL_SOCKET"], row)
            else:
                self.assertEqual(row["HC_TERMINAL_ID"], "t9", row)
        self.assertEqual(sorted({r["cmd"] for r in seen if r["HC_TERMINAL_ID"] is None}), ["keys", "send"])

    def test_closed_tab_ids_are_never_reused(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.assertEqual(self.js("tab", "create", "--workspace", ws)["root_pane"]["pane_id"], f"{ws}:p2")
        self.herdr("tab", "close", f"{ws}:t2")
        self.assertEqual(self.js("tab", "create", "--workspace", ws)["root_pane"]["pane_id"], f"{ws}:p3")

    def test_send_retries_while_the_terminal_is_unavailable(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.clear_calls()
        self.herdr("pane", "run", f"{ws}:p1", "true", env={"FAKE_HC_SEND_UNAVAILABLE": "2"})
        self.assertEqual(len([c for c in self.calls() if c[0] == "send"]), 3)

    def test_send_never_retries_after_the_text_may_have_gone_in(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.clear_calls()
        p = self.herdr("pane", "run", f"{ws}:p1", "echo once", env={"FAKE_HC_SEND_LOST": "1"}, ok=False)
        self.assertEqual((p.returncode, self.error(p)), (1, "unavailable"))
        self.assertEqual(len([c for c in self.calls() if c[0] == "send"]), 1)
        self.assertEqual(self.fake()["terminals"]["t1"]["screen"].count("echo once"), 1)

    def test_only_not_found_means_a_terminal_is_gone(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        p = self.herdr("pane", "get", f"{ws}:p1", env={"FAKE_HC_GET_FAIL": "1"}, ok=False)
        self.assertEqual((p.returncode, self.error(p)), (1, "unavailable"))
        p = self.herdr("agent", "list", env={"FAKE_HC_GET_FAIL": "1"}, ok=False)
        self.assertEqual(p.returncode, 1)
        # A terminal removed between ls and get (not_found) is dropped from the list.
        self.js("tab", "create", "--workspace", ws)
        panes = self.js("pane", "list", "--workspace", ws, env={"FAKE_HC_GET_GONE": "t1"})["panes"]
        self.assertEqual([p["pane_id"] for p in panes], [f"{ws}:p2"])

    def test_wait_output_pauses_when_hc_wait_returns_at_once(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.clear_calls()
        p = self.herdr("pane", "wait-output", f"{ws}:p1", "--regex", "nev.r", "--timeout", "2000", ok=False)
        self.assertEqual(self.error(p), "timeout")
        self.assertLessEqual(len([c for c in self.calls() if c[0] == "wait"]), 5)

    def test_read_shows_no_hc_note_lines(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.edit("t1", screen="line\n")
        self.assertEqual(self.herdr("pane", "read", f"{ws}:p1", "--source", "visible").stdout, "line\n")

    # machines
    def test_machine_list_and_machine_flag(self):
        s = self.fake() if os.path.exists(os.path.join(self.state, "state.json")) else {
            "next": 1, "terminals": {}, "on_send": [], "future": {}}
        s["machines"] = [{"name": "box", "id": "c1", "host": "box", "target": "user@box.example.com",
                          "version": "0.1.22", "state": "connected"}]
        self.put(s)
        rows = json.loads(self.herdr("machine", "list", "--json").stdout)
        self.assertEqual(rows, [{"label": "box", "id": "c1", "target": "user@box.example.com", "host": "box",
                                 "enabled": True, "state": "connected"}])
        r = self.js("--machine", "box", "workspace", "create", "--label", "remote")
        ws = r["workspace"]["workspace_id"]
        run = [c for c in self.calls() if c[0] == "run"][-1]
        self.assertEqual(run[run.index("--on") + 1], "box")
        self.assertEqual(run[run.index("--") + 1], "herdr0")
        self.assertEqual(r["root_pane"]["terminal_id"], "box/t1")
        self.herdr("--machine", "box", "pane", "run", f"{ws}:p1", "true")
        self.assertIn(["send", "--json", "box/t1", "--", "true"], self.calls())
        # Without --machine the remote pane is not this host's.
        p = self.herdr("pane", "get", f"{ws}:p1", ok=False)
        self.assertEqual(self.error(p), "pane_not_found")

    def test_tokens_for_the_taskr_token_daemon(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        self.js("tab", "create", "--workspace", ws)
        self.herdr("workspace", "report-metadata", ws, "--source", "taskr", "--token", "taskr_campaign=7",
                   "--clear-token", "taskr_parent")
        self.assertIn(["label", "--json", "t2", "tok_taskr_campaign=7", "tok_taskr_parent="], self.calls())
        self.assertEqual(self.js("workspace", "list")["workspaces"][0]["tokens"], {"taskr_campaign": "7"})
        self.herdr("pane", "report-metadata", f"{ws}:p1", "--source", "taskr", "--token", "taskr_owner_ask=1")
        panes = {p["pane_id"]: p["tokens"] for p in self.js("pane", "list")["panes"]}
        self.assertEqual(panes[f"{ws}:p1"], {"taskr_campaign": "7", "taskr_owner_ask": "1"})
        self.herdr("pane", "report-metadata", f"{ws}:p1", "--clear-token", "taskr_owner_ask")
        self.assertNotIn("taskr_owner_ask", self.js("pane", "get", f"{ws}:p1")["pane"]["tokens"])

    # refusals
    def test_no_core_refuses_before_any_hc_call(self):
        env = {"HC_SOCKET": os.path.join(self.tmp, "absent.sock")}
        for args in (["agent", "list"], ["pane", "get", "w1:p1"], ["workspace", "create", "--label", "x"],
                     ["machine", "list", "--json"]):
            p = self.herdr(*args, env=env, ok=False)
            self.assertEqual(p.returncode, 3, args)
            self.assertEqual(p.stderr.strip(), "herdr0: no hc core running; the owner starts it")
        self.assertEqual(self.calls(), [])

    def test_socket_check_runs_before_every_hc_call(self):
        ws = self.workspace()["workspace"]["workspace_id"]
        n = len(self.calls())
        # The core goes away after the next call: tab create's second hc call must not run.
        p = self.herdr("tab", "create", "--workspace", ws, env={"FAKE_HC_UNLINK_SOCKET": str(n + 1)}, ok=False)
        self.assertEqual(p.returncode, 3)
        self.assertEqual(len(self.calls()), n + 1)

    def test_default_socket_path(self):
        run = os.path.join(self.tmp, "run")
        os.makedirs(os.path.join(run, "herdr-core"))
        core = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        core.bind(os.path.join(run, "herdr-core", "lab.sock"))
        core.listen(4)
        serve(core)
        try:
            env = {"HC_SOCKET": "", "XDG_RUNTIME_DIR": run, "HC_SESSION": "lab"}
            self.herdr("agent", "list", env=env)
            p = self.herdr("agent", "list", env=dict(env, HC_SESSION="other"), ok=False)
            self.assertEqual(p.returncode, 3)
        finally:
            core.close()

    def test_unsupported_and_no_ops(self):
        for args in (["plugin", "list"], ["plugin", "action", "x"], ["server", "stop"], ["worktree", "list"],
                     ["status"], ["agent", "explain", "impl-a"], ["--skill"]):
            p = self.herdr(*args, ok=False)
            self.assertEqual(p.returncode, 2, args)
            self.assertTrue(p.stderr.startswith("herdr0: unsupported: "), p.stderr)
        self.herdr("notification", "show", "--title", "x", "--body", "y")
        self.assertEqual(self.calls(), [])

    def test_never_runs_itself_as_hc(self):
        p = self.herdr("agent", "list", env={"HC_BIN": os.path.realpath(HERDR0)}, ok=False)
        self.assertEqual(p.returncode, 2)
        self.assertEqual(self.error(p), "hc_is_herdr0")

    def test_help_has_the_root_start_line(self):
        self.assertIn(ROOT_LINE, self.herdr("--help").stdout)

    # enter
    def test_enter_sets_identity_and_execs(self):
        p = self.herdr("enter", "--workspace", "wAB12", "--tab", "wAB12:t3", "--pane", "wAB12:p3", "--",
                       "sh", "-c", 'printf "%s|%s|%s|%s|%s|%s\\n" "$HERDR_ENV" "$HERDR_KIT_BACKEND" '
                       '"$HERDR_WORKSPACE_ID" "$HERDR_TAB_ID" "$HERDR_PANE_ID" "${PATH%%:*}"',
                       env={"HERDR_SOCKET_PATH": "/x"})
        shim = os.path.join(self.tmp, ".local", "lib", "herdr-kit", "hc-bin")
        self.assertEqual(p.stdout.strip(), f"1|hc|wAB12|wAB12:t3|wAB12:p3|{shim}")
        self.assertEqual(self.calls(), [])

    def test_enter_new_workspace_labels_its_own_terminal(self):
        self.put({"next": 2, "on_send": [], "future": {}, "machines": [], "terminals": {"t1": {
            "address": "t1", "name": "root", "argv": [], "cwd": "/w", "labels": {}, "state": "live",
            "screen": "", "pid": 9, "status": None, "turn": 0, "foreground": None}}})
        p = self.herdr("enter", "--new-workspace", "hub", "--", "sh", "-c", 'echo "$HERDR_PANE_ID"',
                       env={"HC_TERMINAL_ID": "t1"})
        lb = self.fake()["terminals"]["t1"]["labels"]
        ws = lb["ws"]
        self.assertRegex(ws, r"^w[0-9A-Z]{4}$")
        self.assertEqual(lb, {"ws": ws, "tab": f"{ws}:t1", "pane": f"{ws}:p1", "tab_label": "1",
                              "ws_label": "hub"})
        self.assertEqual(p.stdout.strip(), f"{ws}:p1")


if __name__ == "__main__":
    unittest.main(verbosity=1)
