<div align="center">

<!-- Banner: generate with Imaginarium when ready — assets/banner.jpg -->

<h1>ApexDesktopHarness-RS</h1>

<p><strong>Agent hands for Linux.</strong><br>
AT-SPI-first desktop control — real eyes and hands for ApexOS-RS agents (and any MCP host),
without defaulting to vision loops.</p>

<p>
<img alt="license" src="https://img.shields.io/badge/license-MIT-blue">
<img alt="rust" src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white">
<img alt="ci" src="https://img.shields.io/github/actions/workflow/status/buckster123/ApexDesktopHarness-RS/ci.yml?label=ci">
<img alt="status" src="https://img.shields.io/badge/status-v0.1%20%C2%B7%20agent--fit-brightgreen">
</p>

</div>

---

> [!NOTE]
> Accessibility tree first, screenshots second. Element actions before coordinate clicks.
> Human physical input always wins. Clean-room design for pure Rust + Linux (Windows later).

## What it is

**Apex Desktop Harness** (`apex-harness`) turns the Linux **AT-SPI2** accessibility tree into a
reliable agent API: discover windows, read compact a11y snapshots, click and type via semantic
actions, and fall back to real mouse/keyboard injection plus scoped screenshots only when the
tree is insufficient. Primary face is MCP-over-stdio for [ApexOS-RS](https://github.com/buckster123/ApexOS-RS);
CLI for humans and scripts. Standalone is first-class.

## Install

```sh
git clone https://github.com/buckster123/ApexDesktopHarness-RS
cd ApexDesktopHarness-RS
cargo build --release --workspace
```

Binaries: `target/release/apex-harness` (CLI) and `target/release/apex-harness-mcp` (MCP).

## Use

```sh
# Readiness (start here)
cargo run -p apex-harness-cli -- doctor

# Eyes
cargo run -p apex-harness-cli -- list-windows
cargo run -p apex-harness-cli -- snapshot --name geany --max-depth 4
cargo run -p apex-harness-cli -- --json find --name geany --role button --max-results 10

# Hands + capture
cargo run -p apex-harness-cli -- --json do-action --id ':1.11|/…' --action Click
cargo run -p apex-harness-cli -- --json screenshot --name geany

# Agent fit
cargo run -p apex-harness-cli -- selftest
cargo run -p apex-harness-cli -- launch org.gnome.Calculator
```

S3 ships wait helpers, launch, selftest, agent skill, and ApexOS integration notes.
See [`BACKLOG.md`](BACKLOG.md) · [`skills/apex-desktop-harness/SKILL.md`](skills/apex-desktop-harness/SKILL.md) ·
[`docs/apexos-integration.md`](docs/apexos-integration.md).

## How it works

```
Agent ── MCP / CLI ──► apex-harness core
                          ├─ A11yBackend   (AT-SPI)
                          ├─ InputBackend  (portal / libei / ydotool / XTest)
                          ├─ CaptureBackend (portal / grim / xcap)
                          └─ WindowBackend (DE-specific + AT-SPI apps)
```

Principles: shell before GUI · a11y before pixels · element actions before coordinates ·
window-scoped capture before full display · audit every mutation · `doctor` first.

## Docs

| File | What's in it |
|------|--------------|
| [`docs/PRD.md`](docs/PRD.md) | Product requirements — goals, architecture, acceptance |
| [`docs/design.md`](docs/design.md) | The contract — tools, types, invariants |
| [`docs/CHARTER.md`](docs/CHARTER.md) | Binding decisions D1–D12 |
| [`docs/apexos-integration.md`](docs/apexos-integration.md) | ApexOS plugins.toml + install |
| [`skills/apex-desktop-harness/SKILL.md`](skills/apex-desktop-harness/SKILL.md) | Agent skill / efficiency rules |
| [`BACKLOG.md`](BACKLOG.md) | Slice ledger — what's shipped, what's next |

## License

MIT — see [LICENSE](LICENSE).

<!-- Banner credit: generate via Imaginarium-RS and note job id here. -->
