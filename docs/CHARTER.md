# ApexCommander-RS — charter

> **The decisions log below is BINDING.** Amend it with a dated entry; never silently.
> Where this document and the code disagree, one of them is a bug — say which.
> Where a later doc and D1–Dn disagree, **D1–Dn win**.

## What this is

**Apex Commander** gives agents real computer-use on a Linux desktop: AT-SPI
accessibility trees for perception and action, real mouse/keyboard injection the human can
see and override, and scoped screenshots only as fallback. Primary face is MCP-over-stdio for
ApexOS-RS `agentd`; CLI and (later) an optional warm daemon serve humans and multi-step speed.
Standalone is first-class — ApexOS is a consumer, never the owner.

## What it is not

- **Not vision-first computer use.** Screenshots and pixel clicks are fallback, never the default.
- **Not a remote desktop / VNC / multi-tenant isolation product.** It drives the real local session.
- **Not a replacement for shell, git, filesystem, or HTTP tools.** Those stay primary when they fit.
- **Not a clone of xfreeze2/desktop-harness or any other product.** Clean-room; inspired by
  capability and philosophy only — no shared code or internal structure.
- **Not OmniOcular-RS.** OmniOcular is multimodal media tools; this harness controls *other apps'* GUIs.
- **Not ApexOS-RS's face / Slint UI verbs.** Those control Apex's own UI; this controls the desktop.
- **Not Windows or macOS in v1** (backends are trait-shaped so Phase 3 is not a rewrite).
- **Not a network service.** No listeners by default; optional daemon is owner-only Unix socket.

## Decisions

- **D1 — Product Apex Commander, repo `ApexCommander-RS` (2026-08-10).** Cargo packages:
  `apex-commander` (core), `apex-commander-cli` (bin `apex-commander`),
  `apex-commander-mcp` (bin `apex-commander-mcp`). Bootstrap titles `ApexDesktopHarness-RS` /
  `apex-harness` are retired. *Why:* digestible product name; garden sibling shape without a
  REST face in v1. *Rules out:* living inside ApexOS-RS workspace on day one.

- **D2 — Clean-room (2026-08-10).** Requirements from the local PRD and public a11y/platform
  docs. Independent architecture, types, and wire formats. No code, schemas, or internals from
  third-party desktop-harness projects.

- **D3 — Three-face shape for v1 (2026-08-10).** Core lib + MCP + CLI. Optional warm daemon and
  REST/API are post-v1. *Why:* PRD integration order; keep Nano surface small. Matches
  Puerperium-style “lib + mcp + cli” when API is not load-bearing.

- **D4 — AT-SPI primary, pixels secondary (2026-08-10).** Perception and action prefer the
  accessibility tree (`atspi` + zbus). Screenshots and coordinate input are explicit fallbacks
  when the tree is empty or insufficient. *Rules out:* vision-loop-default designs.

- **D5 — Backend traits, stable agent API (2026-08-10).** `A11yBackend`, `InputBackend`,
  `CaptureBackend`, `WindowBackend` report capabilities; public tools stay stable across
  GNOME/KDE/Hyprland/i3/Sway and later Windows. *Why:* progressive enhancement without
  rewriting agent skills.

- **D6 — MCP protocol pin: `2024-11-05` (2026-08-10).** Hand-rolled newline-delimited JSON-RPC
  over stdio, no SDK. Lift transport/dispatch patterns from CerebroCortex-RS / OmniOcular-RS.
  stdout sacred; tracing to stderr. Tool failures = MCP `isError` results.

- **D7 — Safety posture (2026-08-10).** Sensitive-app denylist; mutation annotations for
  PolicyEngine; JSONL audit of every mutation; owner-only daemon auth when daemon exists;
  human physical input never blocked; no telemetry; no network exposure by default.

- **D8 — Pure Rust preference (2026-08-10).** Prefer pure-Rust crates (`atspi`, zbus, etc.).
  System tools (`ydotool`, `grim`, compositor CLIs) are allowed as named fallbacks when portals
  or pure paths are incomplete; `doctor` probes and reports them honestly. Sanctioned
  shell-outs are listed under Locked decisions / gotchas as they appear.

- **D9 — Nano / Pi-conscious (2026-08-10).** Suitable for ApexOS Pi-class targets: no heavy
  permanent process unless the daemon is explicitly started; no timeout under 30s for portal
  prompts; never assume accessibility or input groups are already configured — `doctor` says so.

- **D10 — Doctor first (2026-08-10).** `doctor` (and later `selftest`) are first-class. Agents
  verify readiness before multi-step GUI work. S0 ships `doctor` with honest “not wired yet”
  for backends still on the backlog.

- **D11 — Efficiency rules are product law (2026-08-10).** Shell → a11y → element actions →
  coordinates → screenshots. Codified in skill docs and tool descriptions, not only prose PRD.

- **D12 — Licence MIT (2026-08-10).** House default; reassess only if an upstream forces otherwise.

## Phases

| Phase | Scope | Done when |
|-------|-------|-----------|
| **P0 — Bootstrap** | Repo, charter, design, workspace, CI, `doctor` skeleton (env session detect) | `cargo test` green; `apex-commander doctor` prints a structured report; MCP `tools/list` shows `doctor` |
| **P1 — Eyes** | AT-SPI connect, list apps/windows, compact snapshot, find elements | Snapshot of a real GTK/Qt window on this machine; machine-readable JSON |
| **P2 — Hands** | Element actions + input backend + window-scoped screenshot | Click named button + type into field without screenshots in the happy path; audit log line written |
| **P3 — Agent fit** | Full MCP catalog, skill doc, ApexOS `plugins.toml` integration notes, `selftest` | ApexOS agent (or any MCP host) completes the PRD acceptance loop |
| **P4 — Harden** | Presence indicator, warm daemon, Electron a11y polish, install helpers | Field evidence on GNOME + KDE + Hyprland |
| **P5 — Windows** | UI Automation + SendInput backends | Same agent-facing tools on Windows |

## Deliberately out of v1

**Permanently out**

- Telemetry / phoning home — never
- Network-exposed control plane by default — never without an explicit, audited design change
- Shipping third-party proprietary harness code — clean-room only

**Out of v1, honestly deferred**

- Warm daemon (Unix socket + token) — latency optimization once MCP/CLI paths work
- Presence indicator overlay — UX nicety after core hands/eyes
- Windows / macOS backends — Phase 3+
- Batch expression language — discrete tools first (safer for MCP schemas)
- Assimilation as an in-tree ApexOS crate — consumer registers the plugin; ownership stays sibling

## Open questions

1. Cache vs fresh AT-SPI proxies (performance vs correctness).
2. Batch/expression language worth it after discrete tools stabilize?
3. Presence indicator mechanism.
4. Depth of sensitive-app heuristics (static list vs a11y password states).
5. Whether the repo display name shortens (e.g. `ApexHarness-RS`) while keeping crate `apex-commander`.

---

## Amendments

- **2026-08-10** — charter adopted at Launchpad bootstrap (S0).
- **2026-08-10** — product rename to Apex Commander / `ApexCommander-RS` / crates
  `apex-commander*` (D1 amended). Launch branding + public GitHub.
- **2026-08-15** — D7 refined (S5 / `docs/audit-2026-08-15.md`): sensitive-app
  policy matches **window title + app name** (bus-matched windows for element
  ids), never the `{bus}|{path}` id string. Mutating tools and targeted
  perception (`snapshot` / `find` / `screenshot`) **fail closed** when the
  target cannot be classified. Full-display capture and coordinate input use
  **frontmost**. `allow_override` is audited. Audit log must be writable
  before a mutation; typed text never enters audit/result detail. `Protected`
  a11y values are redacted. `field_report` is not read-only (may GrabFocus
  and write a PNG). MCP holds one process-local AT-SPI session (not a daemon).
