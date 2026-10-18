# Working in herdr-kit

These instructions are for agents changing this repository. For installation,
start with [QUICKSTART.md](QUICKSTART.md). Keep each task within its brief.

## Repository slots

- Worktree setup plugin: none. Branch from `origin/main` in a task worktree.
  Docs work needs no dependency installation. Code work uses the Rust toolchain
  in [rust-toolchain.toml](rust-toolchain.toml) and npm version in
  [package.json](package.json); see [README.md](README.md) for build commands.
- Issue tracker: [GitHub Issues for this repository](https://github.com/olafurns7/herdr-kit/issues).
  A task does not grant authority to write to the tracker unless it says so.
- Review skill: [herdr-practices](products/setup/skills/herdr-practices/SKILL.md),
  with [references/review.md](products/setup/skills/herdr-practices/references/review.md).
  The reviewer works independently of the implementer.
- Publish workflow: local gate green, one independent review marked READY at
  the exact proposed commit, then a PR and squash merge. Changes after review
  need review at the new head. Publishing still needs the task's authorization.
  There is no hosted CI by design; pushing does not run the gate.
- Leaf checks: `cargo fmt --all --check` (whole tree, cheap), `git diff --check`
  and `sh products/setup/tests/privacy_check.sh`. No commit hook runs checks.
- Head gate: `sh tools/gate.sh` under slotr with `--kind build`. The gate
  installs the locked npm dependencies itself, so the install step is `true`.

## Skills in briefs

Name the relevant repository skill when assigning changes to its own files:

| Area being changed | Skill to name in the brief |
| --- | --- |
| Orchestration rules and references | [herdr-orchestration](products/setup/skills/herdr-orchestration/SKILL.md) |
| Fleet integration | [herdr-fleet](products/setup/skills/herdr-fleet/SKILL.md) |
| Research, review and writing references | [herdr-practices](products/setup/skills/herdr-practices/SKILL.md) |
| Verification maps and their maintenance | [herdr-verification](products/setup/skills/herdr-verification/SKILL.md) |
| Task ledger and adapters | [taskr](products/taskr/SKILL.md) |

Use the role's loading rules: workers and reviewers do not load the
orchestration or fleet skills merely because those files are in this repo.
The brief should point to the exact source files needed for that change.
Host-specific lanes and account choices belong in private overlays.

## Product layout

- [products/taskr](products/taskr): Rust task ledger and hub daemon, TUI,
  agent adapters, Herdr plugin, installer and product docs.
- [products/slotr](products/slotr): resource admission queue for heavy commands.
- [products/setup](products/setup): public fleet skills, launch helpers,
  integration assets and installer fixtures. taskr owns installation;
  [the setup installer](products/setup/install.sh) is a compatibility wrapper.
- [third_party](third_party): upstream licenses and provenance for adapted text.
  [tools](tools): repository gate and contributor checks.

## Validation

The full local gate is:

```sh
sh tools/gate.sh
```

[The gate](tools/gate.sh) installs the locked npm dependencies, runs Turbo's
Rust build, formatting, tests and clippy (tests and clippy use default and
taskr contract features), release/installer fixtures, setup shell variants, the Node hook
test and privacy check. It uses a scratch HOME and ledger. Budget several
minutes; cold Rust builds and downloads take longer. This is a planning
estimate, not a timing guarantee.

On a shared host use its heavy-command lock. For a configured slotr host:

```sh
slotr run --pool heavy --kind build --campaign YOUR_CAMPAIGN \
  --purpose 'herdr-kit gate' -- sh tools/gate.sh
```

Follow the brief's validation scope; a worker runs the leaf checks and leaves
the gate to the lead, which runs it at the worker's committed head. Report the checked commit and commands.
Focused checks and source inspection do not prove runtime or release behavior.

## Privacy and upstream text

Keep private host names, addresses, user/account details, credentials, live
state and private model policy out of source, examples, fixtures and reports
destined for publication. Use synthetic fixtures and `example.com` or
`hub.example.ts.net` placeholders. Never copy private fleet overlays into source.

```sh
sh products/setup/tests/privacy_check.sh
```

That scanner covers `products/setup`; also inspect every changed file for
private content. A clean denylist scan is not proof that content is public.

When adapting upstream text, record the full source pin, upstream and destination
paths, license and adaptation in `third_party/<source>/PROVENANCE.md`. Preserve the
complete license and ship the notices with the adapted text. Follow the existing
[pstack provenance](third_party/pstack/PROVENANCE.md) and run
[practices_test.sh](products/setup/tests/practices_test.sh) for that pack.

## Releases

Read the product runbook before release work:
[taskr release](products/taskr/docs/release.md) or
[slotr release](products/slotr/docs/release.md). Tags are `taskr/vX.Y.Z` and
`slotr/vX.Y.Z`; each product has its own version. A docs or implementation
assignment grants no release authority.
