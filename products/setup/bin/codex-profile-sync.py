#!/usr/bin/env python3
"""Give every Codator Codex profile the shared ~/.codex configuration.

config.toml stays a private regular file (Codator requires it): shared settings
come verbatim from ~/.codex/config.toml; each profile keeps its own MCP servers,
extra project trust, and extra hook trust. hooks.json and AGENTS.md become links
to the shared files, with hook trust written for the profile's hooks.json path.

Run without flags for a dry run; pass --apply to write. Rerun after Codator adds
a profile or after ~/.codex/config.toml or ~/.codex/hooks.json changes.
"""
import glob, hashlib, json, os, re, sys, tempfile, time, tomllib


NO_MATCHER = {"user_prompt_submit", "stop", "interrupt"}
SESSION_END_LIKE = {"session_end", "interrupt"}
# Codex's session_end/interrupt default timeout is not reproduced here; those
# unchanged hooks copy their trust hash from ~/.codex/config.toml instead.
SESSION_END_DEFAULT_TIMEOUT_SEC = None

def normalized_timeout(event, timeout):
    if event in SESSION_END_LIKE:
        if timeout is None:
            if SESSION_END_DEFAULT_TIMEOUT_SEC is None:
                raise ValueError("session_end default timeout unknown")
            return SESSION_END_DEFAULT_TIMEOUT_SEC
        return timeout
    return max(timeout if timeout is not None else 600, 1)

def hook_hash(event, group, handler):
    """Mirror Codex hook_hash (codex-rs/hooks/src/engine/discovery.rs).

    Checked against the trust hashes Codex stored for unchanged hooks."""
    matcher = None if event in NO_MATCHER else group.get("matcher")
    h = {"type": "command", "command": handler["command"],
         "timeout": normalized_timeout(event, handler.get("timeout")),
         "async": bool(handler.get("async", False))}
    if handler.get("statusMessage") is not None:
        h["statusMessage"] = handler["statusMessage"]
    limit = handler.get("additionalContextLimit")
    if limit is not None and event in {"pre_tool_use","post_tool_use","session_start","user_prompt_submit","subagent_start"} and limit != 2500:
        h["additionalContextLimit"] = limit
    identity = {"event_name": event, "hooks": [h]}
    if matcher is not None:
        identity["matcher"] = matcher
    data = json.dumps(identity, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()
    return "sha256:" + hashlib.sha256(data).hexdigest()

EVENT_KEYS = {"SessionStart":"session_start","SessionEnd":"session_end","Stop":"stop","UserPromptSubmit":"user_prompt_submit",
              "PermissionRequest":"permission_request","PreToolUse":"pre_tool_use","PostToolUse":"post_tool_use",
              "SubagentStart":"subagent_start","SubagentStop":"subagent_stop","Interrupt":"interrupt",
              "PreCompact":"pre_compact","PostCompact":"post_compact"}

def iter_hooks(hooks_json):
    for event_name, groups in hooks_json.get("hooks", {}).items():
        ev = EVENT_KEYS[event_name]
        for gi, group in enumerate(groups):
            for hi, handler in enumerate(group.get("hooks", [])):
                yield ev, gi, hi, group, handler

APPLY = "--apply" in sys.argv
H = os.path.expanduser
GLOBAL = H("~/.codex")
PROFILES = sorted(glob.glob(H("~/.local/share/codator/codex/*/native")))
TS = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
HEADER = re.compile(r'^\s*\[(?!\[).*\]\s*(#.*)?$')

def blocks(text):
    """Split TOML text into (header_path, block_text); the preamble has path None."""
    out, cur, path = [], [], None
    for line in text.splitlines(keepends=True):
        if HEADER.match(line):
            out.append((path, "".join(cur)))
            cur, path = [line], header_path(line)
        else:
            cur.append(line)
    out.append((path, "".join(cur)))
    return out

def header_path(line):
    d, path = tomllib.loads(line.split("#")[0] if '"' not in line else line), []
    while isinstance(d, dict) and len(d) == 1:
        k = next(iter(d)); path.append(k); d = d[k]
    return tuple(path)

def set_trust(text, key, value):
    header = f'[hooks.state."{key}"]'
    lines = text.splitlines(keepends=True)
    for i, line in enumerate(lines):
        if line.strip() == header:
            j = i + 1
            while j < len(lines) and not HEADER.match(lines[j]):
                if lines[j].strip().startswith("trusted_hash"):
                    lines[j] = f'trusted_hash = "{value}"\n'
                    return "".join(lines), "updated"
                j += 1
            lines.insert(i + 1, f'trusted_hash = "{value}"\n')
            return "".join(lines), "added-hash"
    sep = "" if text.endswith("\n\n") else ("\n" if text.endswith("\n") else "\n\n")
    return text + sep + f'{header}\ntrusted_hash = "{value}"\n', "appended"

def trust_entries(hooks_path, hooks, global_states):
    """Hash every shared hook; copy unchanged session_end/interrupt hashes from the global trust."""
    out = {}
    for ev, gi, hi, group, handler in iter_hooks(hooks):
        key = f"{hooks_path}:{ev}:{gi}:{hi}"
        try:
            out[key] = hook_hash(ev, group, handler)
        except ValueError:
            gkey = f"{GLOBAL}/hooks.json:{ev}:{gi}:{hi}"
            out[key] = global_states[gkey]["trusted_hash"]
    return out

def write_private(path, text):
    tomllib.loads(text)  # refuse to write invalid TOML
    if os.path.exists(path) and not os.path.islink(path):
        bak = f"{path}.bak-{TS}"
        with open(bak, "w") as f: f.write(open(path).read())
        os.chmod(bak, 0o600)
    fd, tmp = tempfile.mkstemp(dir=os.path.dirname(path), prefix=".config.toml.")
    with os.fdopen(fd, "w") as f: f.write(text)
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)

def link(path, target):
    if os.path.islink(path) and os.readlink(path) == target:
        return "already linked"
    if os.path.exists(path) or os.path.islink(path):
        if APPLY: os.replace(path, f"{path}.bak-{TS}")
    if APPLY: os.symlink(target, path)
    return f"linked -> {target}"

shared_hooks = json.load(open(f"{GLOBAL}/hooks.json"))
gtext = open(f"{GLOBAL}/config.toml").read()
gcfg = tomllib.loads(gtext)
gstates = gcfg.get("hooks", {}).get("state", {})

# 1. Refresh global trust for the current shared hooks (hook commands may have changed).
changes = []
for key, value in trust_entries(f"{GLOBAL}/hooks.json", shared_hooks, gstates).items():
    if gstates.get(key, {}).get("trusted_hash") != value:
        gtext, how = set_trust(gtext, key, value); changes.append((key.split("hooks.json:")[1], how))
print("global trust changes:", changes or "none")
if APPLY and changes: write_private(f"{GLOBAL}/config.toml", gtext)
gcfg = tomllib.loads(gtext)

for prof in PROFILES:
    # A new profile has no config.toml until something writes one.
    ptext = open(f"{prof}/config.toml").read() if os.path.exists(f"{prof}/config.toml") else ""
    pcfg = tomllib.loads(ptext)
    hooks_path = f"{prof}/hooks.json"
    keep = []
    for path, text in blocks(ptext):
        if not path: continue
        top = path[0]
        # Global MCP servers arrive with gtext; keep only profile-only servers.
        if top == "mcp_servers":
            # The bare [mcp_servers] header already comes with gtext when the global config has one.
            if "mcp_servers" not in gcfg or (len(path) > 1 and path[1] not in gcfg["mcp_servers"]): keep.append(text)
        elif top == "projects" and len(path) > 1 and path[1] not in gcfg.get("projects", {}): keep.append(text)
        elif path[:2] == ("hooks", "state") and len(path) > 2 and path[2] not in gcfg.get("hooks", {}).get("state", {}) and not path[2].startswith(hooks_path): keep.append(text)
        elif top in ("notice", "tui") and top not in gcfg: keep.append(text)
    new = gtext.rstrip("\n") + "\n\n# Profile-local tables (Codator MCP sharing, extra project and hook trust)\n" + "".join(k if k.endswith("\n") else k + "\n" for k in keep)
    for key, value in trust_entries(hooks_path, shared_hooks, gstates).items():
        new, _ = set_trust(new, key, value)
    ncfg = tomllib.loads(new)
    # Profiles align to the global MCP servers and keep any profile-only ones.
    expected_mcp = {**pcfg.get("mcp_servers", {}), **gcfg.get("mcp_servers", {})}
    assert ncfg.get("mcp_servers", {}) == expected_mcp, "MCP servers changed"
    lost_projects = set(pcfg.get("projects", {})) - set(ncfg.get("projects", {}))
    assert not lost_projects, lost_projects
    shared_keys = [k for k in gcfg if k not in ("projects", "hooks", "mcp_servers", "notice", "tui")]
    assert all(ncfg[k] == gcfg[k] for k in shared_keys)
    print(f"{prof.split('/')[-2]}: keep {len(keep)} profile tables, {len(ncfg['hooks']['state'])} hook trust entries, "
          f"mcp={sorted(ncfg.get('mcp_servers', {}))}, projects={len(ncfg.get('projects', {}))}")
    if APPLY: write_private(f"{prof}/config.toml", new)
    print("   hooks.json:", link(hooks_path, f"{GLOBAL}/hooks.json"))
    print("   AGENTS.md: ", link(f"{prof}/AGENTS.md", f"{GLOBAL}/AGENTS.md"))
print("APPLIED" if APPLY else "DRY RUN (no files changed)")
