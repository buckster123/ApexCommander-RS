# ApexDesktopHarness-RS — Agent & Developer Guide

> Agent hands for Linux — AT-SPI-first desktop control (screenshots only as fallback).
> Shape: core lib + MCP stdio face + CLI; optional warm daemon is post-v1.
> ApexOS-RS `agentd` is the first consumer (MCP plugin). **Standalone is first-class.**

Bootstrapped 2026-08-10. House conventions come from `~/Projects/Launchpad-RS/`
— load a doc from there when you need the detail behind a rule below.

**Read `docs/CHARTER.md` before any non-trivial change — its decisions log (D1–D12) is
binding.** Amend it with a dated entry when a decision changes, never silently. Where the
charter and this file disagree, the charter wins.

Siblings: `../ApexOS-RS` (first consumer — `agentd` / PolicyEngine) ·
`../CerebroCortex-RS` (MCP transport pattern to lift) · `../OmniOcular-RS` (multimodal media;
does **not** drive other apps' GUIs) · `../Launchpad-RS` (house bootstrap). Inspiration only
(do **not** copy code): public desktop-harness capability notes referenced from `docs/PRD.md`.

---

## What this is

Coding agents are strong at shell and APIs, weak at real GUIs. This harness turns Linux
**AT-SPI2** accessibility trees into reliable agent eyes and hands: discover windows, read a
compact a11y snapshot, click/type via element actions, fall back to real input injection and
scoped screenshots only when the tree is empty. Visible cursor; human physical input always
wins. Safety: sensitive-app denylist, mutation audit log, policy annotations for ApexOS.

```
crates/
  apex-harness/         # core lib — types, doctor, backends, policy. No I/O glue in faces.
  apex-harness-mcp/     # MCP stdio server — the agent face
  apex-harness-cli/     # clap CLI — binary name `apex-harness`
docs/design.md          # THE contract — tools, types, invariants
docs/CHARTER.md         # binding decisions D1–D12
docs/PRD.md             # product requirements (intent + roadmap)
BACKLOG.md              # slice ledger S0–S4 + post-v1 parking
```

---

## Locked decisions

The load-bearing summary; **`docs/CHARTER.md` D1–D12 is the binding long form.**
**Locked means locked — do not re-litigate these mid-session; amend deliberately, with a date.**

- **Language**: Rust — one Cargo workspace, every binary in it
- **Shape**: standalone sibling (`apex-harness` + `-mcp` + `-cli`); not an ApexOS in-tree crate (D1, D3)
- **Clean-room**: no third-party harness code or schemas (D2)
- **AT-SPI primary, pixels secondary** (D4, D11)
- **Backend traits**, stable agent-facing tools (D5)
- **MCP**: hand-rolled newline-delimited JSON-RPC over stdio, protocol `2024-11-05`, no SDK (D6)
- **Safety**: denylist + audit JSONL + mutation annotations; no network listeners by default (D7)
- **Pure Rust preferred**; named system fallbacks probed by `doctor` (D8)
- **Doctor first** — readiness before multi-step GUI work (D10)
- **CI from commit 0**: fmt `--check` + clippy `-D warnings` + test + build
- **rustfmt-clean baseline from commit 0** — so `cargo fmt --all` is always safe here
- **Licence**: MIT (D12)
- **Nano-conscious**: no heavy permanent process unless daemon is explicit (D9)

---

## The playbook (the house method — read once, then live it)

Full rationale: `~/Projects/Launchpad-RS/docs/house-doctrine.md`. The nine, condensed:

1. **Contract first.** Pin the wire/API/format in `docs/design.md` before code. Code follows
   docs; a PR updates both. **Docs travel with code.**
2. **Slices, not marathons.** One branch = one reviewable slice off freshly-fetched
   `origin/main`. Never open a PR whose base is another branch.
3. **Honesty invariants.** Never a fake success. Degrades are *stated*. Failures carry the
   real reason. Never silently clamp what you can honestly reject.
4. **Pure-fn test discipline.** Tree compactors, selectors, policy matchers, report builders
   are the unit-test surface; D-Bus/portal handlers are thin glue. Effectful e2e tests skip *loudly*.
5. **Field truth beats green CI.** A slice is done when it runs on a live desktop session —
   real window, real click — not when tests pass. The ledger row gets its ✅ only then.
6. **Secrets hygiene.** Never print a key or token (lengths and heads only). Never write one
   into a repo, a transcript, a doc, or a non-0600 file. **No credentials in CLAUDE.md.**
7. **Cerebro is the thread.** `session_recall` at start, `session_save` at milestones and end.
8. **Spend is gated.** Paid operations never auto-fire. Live-fire is André's call.
9. **Cost the failure, not the happy path.** Long pending work stays recoverable; never orphan spend.

---

## Git discipline

- **Never commit to `main`.** Feature branch off freshly-fetched `origin/main`: `feat/…`,
  `fix/…`, `chore/…`, `docs/…`. One branch = one slice.
- **Ship via PR** (`gh pr create`). **Do NOT merge it yourself** — André reviews and merges,
  or explicitly tells you to. (Pre-publication bootstrap commits are the sanctioned exception.)
- **Commit format:** imperative, lowercase. End with the `Co-Authored-By` trailer.
- **Never amend a pushed commit. Never force-push.**
- **Push after every commit.** Local git is the floor of resilience: if Cerebro is
  unavailable, the repo + its docs must be enough to reconstruct full project context.

---

## Cerebro session protocol (mandatory)

All Cerebro MCP calls use agent `FORGE` (`agent_id="FORGE"`) — or the agent actually doing
the work; keep tags `project:ApexDesktopHarness-RS`. Full tool menu:
`~/Projects/Launchpad-RS/docs/cerebro-protocol.md`.

**Session START** — before touching any code:
```
session_recall(query="ApexDesktopHarness-RS build status step progress", agent_id="FORGE")
```

**Session END** (and at milestones on long sessions):
```
session_save(session_summary="what was built, what broke, what was learned",
             key_discoveries=[...], unfinished_business=[...],
             agent_id="FORGE", priority="HIGH")
```

**The vaults:** CLAUDE.md = lean core + pointers · `docs/gotchas.md` = invariants ·
`docs/*.md` = per-topic detail · Cerebro = session memory · git = code truth.

---

## Dev commands

```bash
cargo test --workspace
cargo fmt --all && cargo clippy --workspace -- -D warnings
cargo build --release --workspace

# Human face — eyes + hands (S1–S2)
cargo run -p apex-harness-cli -- doctor
cargo run -p apex-harness-cli -- list-windows
cargo run -p apex-harness-cli -- snapshot --name geany --max-depth 4
cargo run -p apex-harness-cli -- --json find --name geany --role button --max-results 10
cargo run -p apex-harness-cli -- --json screenshot --full
cargo run -p apex-harness-cli -- selftest
cargo run -p apex-harness-cli -- field-report --markdown
./scripts/run-field-matrix.sh   # saves docs/field-evidence/<session>.json
cargo run -p apex-harness-cli -- launch org.gnome.Calculator
# do-action / type-into need a live element id from find

# MCP smoke (stdout is JSON-RPC only)
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_windows","arguments":{}}}' \
  | cargo run -q -p apex-harness-mcp
```

No systemd unit in v1 (not a permanent daemon). When the optional daemon lands, follow
`~/Projects/Launchpad-RS/docs/deploy.md`.

---

## Gotchas

Project invariants live in **`docs/gotchas.md`** — grep it before modifying a subsystem.
Cross-project version drift is in `~/Projects/Launchpad-RS/docs/sharp-edges.md`.

Two that bite every project in this garden:

- **MCP stdout is sacred.** All `tracing`/log output goes to **stderr**. A stray `println!`
  corrupts the JSON-RPC stream.
- **Read the pinned crate's docs for the exact version** — not memory of an older API.

Project-specific (will grow in S1+):

- AT-SPI needs a live session bus; headless CI will not exercise real a11y — keep pure logic
  under unit tests and mark field tests so they skip loudly without a display.
- Wayland input often needs XDG RemoteDesktop / libei portal consent; document and probe,
  never assume.

---

## Docs

| File | Load when working on |
|------|----------------------|
| `docs/CHARTER.md` | **Binding decisions D1–D12, phases, scope fence** |
| `docs/design.md` | **The contract** — tools, types, invariants |
| `docs/PRD.md` | Product intent, acceptance criteria, roadmap narrative |
| `docs/apexos-integration.md` | plugins.toml + install path for ApexOS-RS |
| `docs/field-matrix.md` | GNOME/Plasma/Hyprland field ledger |
| `skills/apex-desktop-harness/SKILL.md` | Agent efficiency rules + tool cheatsheet |
| `docs/gotchas.md` | **Any subsystem change — grep it first** |
| `BACKLOG.md` | Outstanding work — slice ledger + parked items |

---

## Meta — when to update this file

- A locked decision changes → **`docs/CHARTER.md` first** (dated amendment), then the summary here
- A gotcha is discovered → **`docs/gotchas.md`**, not here
- A slice completes → tick it in `BACKLOG.md`
- A doc file is created → add a row to `## Docs`
- **Keep this file under ~250 lines / ~20 KB.** Fat goes to `docs/`; this file points.
- Before publishing the repo, inline anything it truly depends on from `Launchpad-RS/` so the
  repo stands alone for outside readers.

### What never goes in CLAUDE.md or docs/*.md

- Task progress, session logs, completed-work summaries → Cerebro (`session_save`)
- Git SHAs, version pins → stale in days, belong in git history
- Commentary on what you just did → belongs in commit messages
- **Credentials of any kind** → env files (0600), never a tracked file
