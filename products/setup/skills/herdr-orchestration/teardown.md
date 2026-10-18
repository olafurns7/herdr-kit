# Teardown: host capacity and per-worker teardown

Load when: before stopping an owned worker, before closing a tab or workspace, after collecting a worker's result, and after a reboot or crash (before resuming anything; then `recovery.md`). Before adding a heavy agent, load the core's Preflight for capacity limits. Part of herdr-orchestration; `SKILL.md` is the core.

## Host capacity

Use `SKILL.md` Preflight for the memory check and heavy-agent limits.

## Per-worker teardown

- After collecting an owned worker's report and result, verify the live agent identity before quitting only that worker:

```bash
herdr agent get impl-<slug>
herdr agent send-keys impl-<slug> esc            # Codex: leave any dialog
herdr agent send-keys impl-<slug> ctrl+c ctrl+c  # request exit; inspect for a dialog
herdr agent list                                 # confirm it is gone
```

- A Claude worker with a background shell task or Monitor may show an "Exit and stop tasks" dialog after `ctrl+c ctrl+c`. Inspect with `herdr pane read <pane> --source visible --lines 30`. Only when it shows that exact dialog with option 1 highlighted, confirm with `herdr pane send-keys <pane> Enter`. For any other dialog, stop and inspect; never send blind input.

- Before closing a helper's tab, verify that its pane still belongs to the recorded task workspace, that its foreground process is the interactive shell, and that its output shows the shell prompt. Also confirm that another tab remains in the task workspace:

```bash
helper=$(herdr pane get "$helper_pane_id")
helper_workspace_id=$(printf '%s\n' "$helper" | jq -er '.result.pane.workspace_id')
test "$helper_workspace_id" = "$task_workspace_id"
herdr pane process-info --pane "$helper_pane_id"
herdr pane read "$helper_pane_id" --source recent-unwrapped --lines 40
herdr tab get "$helper_tab_id"
herdr tab list --workspace "$task_workspace_id"
```

Close only a tab id you recorded at launch (`helper_tab_id` in `launch.md`). Never pick Herdr ids with `grep` on Herdr JSON: `herdr tab list` prints one JSON line, so `grep` matches the whole line and `grep -o` returns every tab id on the host. When you must derive ids, filter the exact field with jq, then print the list and check it before any close:

```bash
herdr tab list | jq -r --arg ws "$task_workspace_id" '.result.tabs[] | select(.workspace_id == $ws) | .tab_id'
```

Only then close the helper's tab:

```bash
herdr tab close "$helper_tab_id"
taskr close <task> --outcome <outcome>             # ledger only, after the teardown
```

`accepted` means taken as delivered, `reworked` taken after a fix round, `rejected` not taken, and `abandoned` stopped before a result. Always give an outcome when a lane produced or was meant to produce work; gate and consult lanes close without one.

Closing the last tab of a workspace closes the workspace. Do not trigger a cascading closure by accident. When the finished helper holds the last tab, leave it at the shell prompt; the next helper in that workspace may reuse it under `launch.md`, and the tab closes with the task workspace.

## Inventory before cleanup

Run `herdr-worktrees [REPO]` (or `herdr-worktrees --json [REPO]`) before cleanup. It reads local refs only; ahead/behind and merged evidence may be stale or unknown. Check the task by hand with `taskr status --all` (the lane's workspace identifies it).

`merged` is local content evidence, not removal permission. Removal still needs the task closed, ownership checks, and plain `git worktree remove` (never `--force`). Review and preserve ignored entries: Git can delete them even without `--force`. Anything `dirty`, `unpushed`, `open`, `missing`, or unknown needs its owner's decision; `current` stays in use. Cleanup belongs to whole-task completion.

## Workspace lifecycle, reboot, and crash

- Do not close a workspace or remove a worktree when one worker finishes. Workspace and worktree lifecycle belongs to whole-task completion, subject to ownership and preservation of changes. Never add `--force` or `--group` for cleanup.
- After a reboot or crash: run `herdr agent list` and `ps -A -o rss=,pid=,comm= | sort -rn | head` (POSIX `ps`, so it also works on macOS), tear down stranded sessions first, then resume work.
- Never close workspaces you did not create, and never run `herdr server stop` during active work.
