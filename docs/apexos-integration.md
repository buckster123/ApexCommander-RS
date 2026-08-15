# ApexOS-RS integration notes

Apex Commander is a **standalone sibling** (not an ApexOS workspace member).
ApexOS-RS consumes it as an MCP stdio plugin the same way it consumes Occipital / Sonus.

## Build & install (manual)

```sh
cd ~/Projects/ApexCommander-RS
cargo build --release --workspace
# install binaries where agentd can exec them
sudo install -m 755 target/release/apex-commander-mcp /usr/local/bin/apex-commander-mcp
sudo install -m 755 target/release/apex-commander     /usr/local/bin/apex-commander
```

On a development box you may point `cmd` at the cargo target path instead.

## `plugins.toml` block (template)

Append to `/etc/agentd/plugins.toml` (or the agentd config path used by the node).
**Keep commented until the binary is installed** — same house pattern as Occipital/Sonus.

```toml
# Apex Commander — AT-SPI-first computer use (Linux).
# Standalone sibling: github.com/buckster123/ApexCommander-RS
# Not built by ApexOS `cargo build --workspace`. Install apex-commander-mcp separately.
#
# [[plugin]]
# id      = "apex-commander"
# cmd     = "/usr/local/bin/apex-commander-mcp"
# args    = []
# restart = "on-failure"
# [plugin.env]
# RUST_LOG = "warn"
# # APEX_COMMANDER_STATE_DIR  = "/var/lib/agentd/apex-commander"
# # APEX_COMMANDER_CONFIG_DIR = "/etc/apex-commander"
```

Notes:

- **stdout is JSON-RPC only** — never set log formats that write to stdout.
- Prefer `restart = "on-failure"` over `always` for a cold-spawn-friendly tool plugin;
  agentd may also spawn on demand depending on host version.
- State dir holds `audit.jsonl` + screenshots; on multi-user nodes keep it under the
  agentd data root with tight permissions.
- Secrets never go in this file. Sensitive denylist config:
  `$APEX_COMMANDER_CONFIG_DIR/sensitive.toml`.

## PolicyEngine

Mutating MCP tools advertise:

- `readOnlyHint: false` for `do_action`, `click_element`, `type_into`, `set_value`, `activate`,
  `launch`, `mouse_*`, `type_text`, `key`, `selftest`, `field_report`
- `destructiveHint: true` for `type_into`, `type_text`, `key`, `launch` (secrets / submit / spawn)
- `readOnlyHint: true` for `doctor`, `list_*`, `snapshot`, `find_*`, `wait*`, `screenshot`

Route high-risk tools through the existing approval UI. The harness **also** applies a
local sensitive-app denylist; that is a second line of defense, not a replacement for
PolicyEngine.

## Agent skill

Ship or symlink:

```
ApexCommander-RS/skills/apex-commander/SKILL.md
```

into the agent's skill search path (Hermes/Apex skill dirs as appropriate). The skill
encodes the efficiency rules (shell → a11y → element actions → coordinates → screenshots).

## Acceptance smoke (agent or human)

```sh
apex-commander doctor
apex-commander selftest                 # non-mutating
apex-commander selftest --confirm       # + mouse wiggle if ydotool/xdotool present

# PRD happy path without screenshots:
apex-commander list-windows
apex-commander snapshot --name geany --max-depth 4
apex-commander --json find --name geany --role button --element-name New
apex-commander --json do-action --id '…' --action Click
# type_into when a text field id is known
# screenshot only as fallback
```

## Optional: local MCP for Claude/Grok on the workstation

```json
{
  "mcpServers": {
    "apex-commander": {
      "command": "/home/andre/Projects/ApexCommander-RS/target/release/apex-commander-mcp",
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
