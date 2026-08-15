<div align="center">

<img src="assets/banner.jpg" alt="Apex Commander" width="100%">

# Apex Commander

### Desktop hands for your agents.

Apex Commander gives AI agents (and you) reliable control of real Linux apps —  
**see what’s on screen through the accessibility tree, click and type like a human,**  
and fall back to screenshots only when the tree isn’t enough.

<p>
  <a href="LICENSE"><img alt="License MIT" src="https://img.shields.io/badge/license-MIT-blue?style=flat-square"></a>
  <img alt="Rust 2021" src="https://img.shields.io/badge/rust-2021-orange?style=flat-square&logo=rust&logoColor=white">
  <img alt="Linux" src="https://img.shields.io/badge/platform-Linux%20(Wayland%20%2F%20X11)-brightgreen?style=flat-square&logo=linux&logoColor=white">
  <img alt="AT-SPI first" src="https://img.shields.io/badge/perception-AT--SPI%20first-0ea5e9?style=flat-square">
  <a href="https://github.com/buckster123/ApexCommander-RS/actions"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/buckster123/ApexCommander-RS/ci.yml?style=flat-square&label=ci"></a>
  <img alt="Status" src="https://img.shields.io/badge/status-v0.1%20field-brightgreen?style=flat-square">
</p>

<p>
  <a href="#quick-start"><strong>Quick start</strong></a> ·
  <a href="#what-it-does"><strong>What it does</strong></a> ·
  <a href="#for-agents"><strong>For agents</strong></a> ·
  <a href="#how-it-works"><strong>How it works</strong></a> ·
  <a href="#docs"><strong>Docs</strong></a>
</p>

</div>

---

## Why this exists

Agents are great at terminals, files, and APIs. They’re still clumsy with **real desktop apps**.

Most “computer use” stacks default to **screenshot → vision model → pixel click**. That works, but it’s slow, expensive, brittle, and hard to audit.

Linux already exposes a better signal: the **AT-SPI accessibility tree** — structured roles, names, states, and actions that GTK, Qt, browsers, and many other toolkits publish.

**Apex Commander** turns that into a clean CLI + MCP surface your agent can call safely.

> Prefer structure over pixels. Prefer element actions over coordinates.  
> Keep the human able to grab the mouse at any time.

---

## What it does

| Capability | In practice |
|------------|-------------|
| **Discover** | List apps and windows; find the frontmost surface |
| **See** | Compact accessibility snapshots and semantic find (`button` + `"OK"`) |
| **Act** | Click, type, and set values via AT-SPI actions first |
| **Wait** | Poll for an element or a stable UI tree between steps |
| **Launch** | Open apps by desktop id (`org.gnome.Calculator`) |
| **Fallback** | Window-scoped screenshots + optional coordinate input when the tree is empty |
| **Safety** | Mutation audit log, sensitive-app denylist, policy-friendly tool annotations |

Built as a **standalone** pure-Rust project. First-class consumer: [ApexOS-RS](https://github.com/buckster123/ApexOS-RS) via MCP. Works with any MCP host or shell agent.

---

## Quick start

### Requirements

- Linux desktop session (Wayland or X11)
- Rust toolchain (edition 2021)
- AT-SPI available (normal on GNOME/KDE; enable toolkit a11y on minimal setups)
- Optional: `ydotool` / `wtype` for coordinate typing; portal stack for screenshots

### Install

```sh
git clone https://github.com/buckster123/ApexCommander-RS
cd ApexCommander-RS
cargo build --release --workspace
```

Binaries land at:

- `target/release/apex-commander` — human / ops CLI  
- `target/release/apex-commander-mcp` — MCP server (stdio)

### 30-second smoke

```sh
# Is this machine ready?
./target/release/apex-commander doctor

# Structured field report for your DE
./target/release/apex-commander field-report --markdown

# Non-destructive self-test
./target/release/apex-commander selftest

# See what’s open
./target/release/apex-commander list-windows
```

### A tiny real workflow

```sh
# Launch a simple app
./target/release/apex-commander launch org.gnome.Calculator

# Wait until a button appears, then list matches
./target/release/apex-commander wait-for-element --name Calculator --role button

# Snapshot the accessibility tree (no vision model required)
./target/release/apex-commander snapshot --name Calculator --max-depth 4

# Prefer: do-action with an element id from find (not coordinates)
./target/release/apex-commander --json find --name Calculator --role button --element-name "1"
# ./target/release/apex-commander --json do-action --id ':1.x|/…' --action Click
```

---

## For agents

### Efficiency rules (load these)

1. **Shell first** — don’t open a GUI for what the terminal already does.  
2. **AT-SPI before screenshots** — `snapshot` / `find_elements` before `screenshot`.  
3. **Element actions before coordinates** — `do_action` / `type_into` before `mouse_click`.  
4. **Window capture before full display** when you must screenshot.  
5. **Ask before high-risk actions** — messages, payments, deletes, credentials.  
6. **Run `doctor` (or `selftest`) when anything looks off.**

Full agent skill: [`skills/apex-commander/SKILL.md`](skills/apex-commander/SKILL.md)

### MCP (stdio)

```sh
# tools/list smoke
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | ./target/release/apex-commander-mcp
```

Register with ApexOS-RS: see [`docs/apexos-integration.md`](docs/apexos-integration.md).

**23 tools** including discover, snapshot, find, do_action, click_element, type_into, wait_*, launch, screenshot, selftest, field_report.

---

## How it works

```
Agent / human
    │
    ├─ MCP stdio   (apex-commander-mcp)
    └─ CLI         (apex-commander)
            │
            ▼
     apex-commander core
            │
   ┌────────┼────────┬────────────┐
   ▼        ▼        ▼            ▼
 A11y    Input    Capture      Window
 (AT-SPI) (ydotool…) (portal/…)  (via a11y)
```

**Philosophy:** progressive enhancement. A session with only AT-SPI is already useful. Portals, grim, ydotool, and compositor helpers light up when present — and **`doctor` / `field-report` say so honestly** when they’re not.

### Field status (v0.1)

| Desktop | Status |
|---------|--------|
| GNOME Wayland | **PASS** (live matrix on Ubuntu) |
| KDE Plasma | Protocol ready — re-run `./scripts/run-field-matrix.sh` in a Plasma session |
| Hyprland | Protocol ready — same script once Hyprland is installed |

Details: [`docs/field-matrix.md`](docs/field-matrix.md)

---

## Safety & privacy

- **No network listeners** by default  
- **Audit trail** of mutations under `~/.local/share/apex-commander/audit.jsonl`  
- **Sensitive-app denylist** by window title + app name (not element ids); fail closed if unclassified  
- **Human override always wins** — real mouse/keyboard is never blocked  
- Tool annotations for host PolicyEngines (`readOnlyHint` / `destructiveHint`)  
- Audit JSONL never stores typed text (character count only); `Protected` values are redacted

---

## Docs

| Doc | Audience |
|-----|----------|
| [`skills/apex-commander/SKILL.md`](skills/apex-commander/SKILL.md) | Agents — when/how to use tools |
| [`docs/design.md`](docs/design.md) | Implementers — API contract |
| [`docs/CHARTER.md`](docs/CHARTER.md) | Binding product decisions |
| [`docs/PRD.md`](docs/PRD.md) | Product requirements |
| [`docs/apexos-integration.md`](docs/apexos-integration.md) | ApexOS `plugins.toml` |
| [`docs/field-matrix.md`](docs/field-matrix.md) | Compositor field ledger |
| [`docs/gotchas.md`](docs/gotchas.md) | Load-bearing invariants |
| [`BACKLOG.md`](BACKLOG.md) | Slice ledger |

Developer guide for contributors/agents working *in* this repo: [`CLAUDE.md`](CLAUDE.md)

---

## Project shape

```
crates/
  apex-commander/       # core library
  apex-commander-cli/   # CLI binary: apex-commander
  apex-commander-mcp/   # MCP binary: apex-commander-mcp
skills/apex-commander/  # agent skill
docs/                   # charter, design, field matrix, integration
```

---

## License

MIT — see [LICENSE](LICENSE).

---

<sub>
Banner generated with <a href="https://github.com/buckster123/Imaginarium-RS">Imaginarium-RS</a>
(job <code>01KZPH81A0VNFPGQ1EPJT1F8B6</code>, Grok Imagine quality).
Part of the <a href="https://github.com/buckster123">buckster123</a> <code>*-RS</code> garden —
sibling to <a href="https://github.com/buckster123/ApexOS-RS">ApexOS-RS</a>.
</sub>
