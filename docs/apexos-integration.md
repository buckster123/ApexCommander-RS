# ApexOS-RS integration notes

Apex Desktop Harness is a **standalone sibling** (not an ApexOS workspace member).
ApexOS-RS consumes it as an MCP stdio plugin the same way it consumes Occipital / Sonus.

## Build & install (manual)

```sh
cd ~/Projects/ApexDesktopHarness-RS
cargo build --release --workspace
# install binaries where agentd can exec them
sudo install -m 755 target/release/apex-harness-mcp /usr/local/bin/apex-harness-mcp
sudo install -m 755 target/release/apex-harness     /usr/local/bin/apex-harness
```

On a development box you may point `cmd` at the cargo target path instead.

## `plugins.toml` block (template)

Append to `/etc/agentd/plugins.toml` (or the agentd config path used by the node).
**Keep commented until the binary is installed** — same house pattern as Occipital/Sonus.

```toml
# Apex Desktop Harness — AT-SPI-first computer use (Linux).
# Standalone sibling: github.com/buckster123/ApexDesktopHarness-RS
# Not built by ApexOS `cargo build --workspace`. Install apex-harness-mcp separately.
#
# [[plugin]]
# id      = "apex-harness"
# cmd     = "/usr/local/bin/apex-harness-mcp"
# args    = []
# restart = "on-failure"
# [plugin.env]
# RUST_LOG = "warn"
# # APEX_HARNESS_STATE_DIR  = "/var/lib/agentd/apex-harness"
# # APEX_HARNESS_CONFIG_DIR = "/etc/apex-harness"
```

Notes:

- **stdout is JSON-RPC only** — never set log formats that write to stdout.
- Prefer `restart = "on-failure"` over `always` for a cold-spawn-friendly tool plugin;
  agentd may also spawn on demand depending on host version.
- State dir holds `audit.jsonl` + screenshots; on multi-user nodes keep it under the
  agentd data root with tight permissions.
- Secrets never go in this file. Sensitive denylist config:
  `$APEX_HARNESS_CONFIG_DIR/sensitive.toml`.

## PolicyEngine

Mutating MCP tools advertise:

- `readOnlyHint: false` for `do_action`, `type_into`, `set_value`, `activate`, `launch`,
  `mouse_*`, `type_text`, `key`, `selftest` (when mutate confirmed)
- `readOnlyHint: true` for `doctor`, `list_*`, `snapshot`, `find_*`, `wait*`, `screenshot`

Route high-risk tools through the existing approval UI. The harness **also** applies a
local sensitive-app denylist; that is a second line of defense, not a replacement for
PolicyEngine.

## Agent skill

Ship or symlink:

```
ApexDesktopHarness-RS/skills/apex-desktop-harness/SKILL.md
```

into the agent's skill search path (Hermes/Apex skill dirs as appropriate). The skill
encodes the efficiency rules (shell → a11y → element actions → coordinates → screenshots).

## Acceptance smoke (agent or human)

```sh
apex-harness doctor
apex-harness selftest                 # non-mutating
apex-harness selftest --confirm       # + mouse wiggle if ydotool/xdotool present

# PRD happy path without screenshots:
apex-harness list-windows
apex-harness snapshot --name geany --max-depth 4
apex-harness --json find --name geany --role button --element-name New
apex-harness --json do-action --id '…' --action Click
# type_into when a text field id is known
# screenshot only as fallback
```

## Optional: local MCP for Claude/Grok on the workstation

```json
{
  "mcpServers": {
    "apex-harness": {
      "command": "/home/andre/Projects/ApexDesktopHarness-RS/target/release/apex-harness-mcp",
      "args": []
    }
  }
}
```

Point at `target/release/…` after `cargo build --release` so agents do not run a stale binary.

## What stays out of ApexOS

- No in-tree crate membership required for v1
- No shared process with agentd beyond MCP stdio
- Assimilation into the ApexOS workspace is a later, deliberate decision (charter D1)
