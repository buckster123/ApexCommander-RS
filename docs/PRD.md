**Apex Desktop Harness (ADH)**  
**Clean-room Product Requirements Document**  
Version 0.1 · 2026-08-10  
Status: Ready for implementation planning  

**Working title / binary:** `apex-harness` (crate `apex-desktop` or `apex-harness`)  
**Tagline:** Agent hands for Linux (Windows later) — AT-SPI-first desktop control that gives ApexOS-RS agents real computer-use powers.

This is an independent clean-room design. It is inspired by the *capabilities and philosophy* of [xfreeze2/desktop-harness](https://github.com/xfreeze2/desktop-harness) (macOS AX-tree-first CLI + skill for agents) but shares no code, no exact internal structure, and is re-architected for pure Rust, Linux (primary), and deep integration with [buckster123/ApexOS-RS](https://github.com/buckster123/ApexOS-RS).

---

### 1. Overview & Problem Statement

Coding and embodied agents are strong at shell, files, git, and APIs, but weak at real GUI applications. Screenshot → vision → coordinate click loops are capable yet slow, expensive, brittle, and token-heavy.

macOS already exposes a rich Accessibility (AX) tree. Linux exposes the equivalent via **AT-SPI2** (Assistive Technology Service Provider Interface) over D-Bus. Most GTK, Qt, Firefox, Electron (with accessibility enabled), and many other toolkits publish structured role/name/value/state/action information.

**Apex Desktop Harness** turns that into reliable agent “eyes + hands”:

- Prefer the accessibility tree for perception and actions.
- Fall back to scoped screenshots only when the tree is empty or insufficient (custom canvases, games, poorly instrumented apps).
- Inject real system-level mouse and keyboard events so the human can see the pointer move and can always override.
- Expose a clean, schema-driven surface that ApexOS-RS agents (and any other MCP or shell agent) can call safely and efficiently.

The result is fast multi-step GUI workflows without defaulting to vision models.

---

### 2. Goals

1. Give ApexOS-RS agents (and any MCP/shell agent) first-class computer-use on a real Linux desktop.
2. Accessibility-tree primary, screenshots secondary.
3. Real input injection (visible cursor, physical override always wins).
4. Pure Rust, low latency, optional long-lived process for multi-step speed.
5. Native fit for ApexOS-RS: MCP-over-stdio plugin (like `apexos-tools` / `cerebro-mcp`), plus CLI, plus optional daemon.
6. Strong safety, auditability, and policy hooks suitable for a self-evolving embodied agent that already has hardware and self-update powers.
7. Progressive support across major Linux sessions (X11 + Wayland, major DEs/compositors).
8. Clear doctor / selftest path so agents and humans can verify readiness.
9. Windows (UI Automation + SendInput) as a later phase; design the trait boundaries so it is not a rewrite.

### Non-Goals

- Vision-first / pixel-click loops as the default path.
- Full remote-desktop / VNC / multi-user isolation by default (this drives the *real* desktop of the machine the agent lives on).
- Replacing shell, git, filesystem, or HTTP tools.
- Cloning any existing computer-use product or its code.
- Guaranteeing perfect a11y coverage for every Electron/WebKit app (fallback exists).

---

### 3. Target Users & Integration with ApexOS-RS

**Primary consumer:** `agentd` inside ApexOS-RS.  
The harness is registered as an MCP plugin (stdio JSON-RPC) in `plugins.toml`. Agentd spawns it on demand (or keeps a warm instance). High-risk tools route through the existing PolicyEngine / approval UI. Results and screenshots can be ingested into Cerebro.

**Secondary consumers:**
- Any MCP host (Claude Code, Codex, custom agents).
- Shell / scripting agents that call the CLI.
- Human operators using the CLI for debugging or one-off automation.

**Integration shape (preferred order):**
1. MCP server mode (stdio) — primary for ApexOS.
2. CLI binary with structured subcommands + optional JSON/batch mode.
3. Optional long-running user daemon (Unix socket + token, owner-only) that caches AT-SPI proxies and input devices for lower multi-step latency.
4. Thin Rust library surface so other Apex crates can link it directly if desired.

An agent skill / documentation file (analogous to the original `SKILL.md`) will teach agents *when* and *how* to use the tools, with the same efficiency rules: shell first, a11y before pixels, element actions before coordinates, ask before outbound/destructive actions.

Existing Apex tools (`screenshot_mirror`, camera, `run_command`, UI verbs for the Slint face) remain complementary; this harness controls *other* applications’ GUIs.

---

### 4. Core Principles (Non-Negotiable)

1. **Shell / native tools before GUI.**
2. **AT-SPI tree before screenshots.**
3. **Element-level actions (`do_action`, set value) before coordinate clicks.**
4. **Window-scoped capture before full-display capture.**
5. **No vision loop by default.**
6. **Consent / policy before high-risk mutations** (messages, payments, deletes, credential surfaces, security settings).
7. **Human physical input always wins.**
8. **Audit every mutation.**
9. **Doctor first** — agents and humans must be able to verify the environment quickly.

---

### 5. Functional Requirements

#### 5.1 Diagnostics & Setup
- `doctor` → structured readiness report (session type X11/Wayland, AT-SPI bus live, input backend available, portals, compositor window backend, sensitive-app list, recommended next steps).
- `setup_accessibility` / `setup_input` helpers (enable toolkit a11y where needed, document uinput / input-group / portal requirements).
- `selftest` — non-destructive smoke tests that exercise discovery, tree read, and (with confirmation) a visible mouse wiggle + type into a safe target.

#### 5.2 Discovery
- List running applications and top-level windows (name, PID, class, bounds, focused state, a11y root if available).
- Frontmost / focused window and application.
- Activate / focus / raise a window or application by name, ID, or PID.
- Open / launch an application by desktop entry name or executable (with safety checks).

#### 5.3 Perception (Eyes)
- Compact accessibility snapshot of a window or the focused application: role, name/title, description, value, states, actions, bounds, children (depth- and node-budget limited, filterable).
- Find elements by role + name / text / state / description (semantic selectors).
- Focused element details.
- Screenshot (window, region, or full display) returning a path or base64; prefer portal / efficient backends; optional annotation of a11y bounds for debugging.
- Optional media / player state helpers if useful on Linux.

#### 5.4 Action (Hands)
- Prefer AT-SPI `do_action` (click/press/activate/etc.) and value-setting interfaces.
- Semantic helpers: `click_element` / `click_text`, `set_value` / `type_into`, `perform_action`.
- Coordinate / real-input fallback: mouse move (absolute/relative), click (left/right/middle, single/double), drag, scroll, type text (layout-aware where possible), key / hotkey.
- Wait / wait-for-stable / wait-for-element helpers.

#### 5.5 Meta & Presence
- Optional visual presence indicator (soft ring or pill) so a human watching the screen knows the agent currently owns the pointer.
- Batch / multi-step execution support (especially useful in daemon or MCP multi-call scenarios).
- Clear success / failure / ambiguity reporting so agents can recover (e.g., “multiple matches — take a fresh snapshot”).

All mutating tools must be clearly annotated (read-only vs mutating vs destructive) so Apex’s PolicyEngine and other MCP hosts can require approval.

---

### 6. Non-Functional Requirements

- **Latency:** Tree queries and element actions in low tens of ms when warm; first call after cold start acceptable higher.
- **Reliability:** Graceful degradation across compositors; explicit capability reporting in `doctor`.
- **Resource use:** Suitable for Pi-class hardware that ApexOS targets; avoid heavy permanent processes unless the daemon is explicitly started.
- **Security:** See section 8. No network listeners by default. Owner-only sockets/tokens. No telemetry.
- **Observability:** JSONL audit log of mutations (who/what/when/target). Structured logging.
- **Testability:** `doctor` + `selftest` must be runnable headlessly where possible and produce machine-readable output.
- **Extensibility:** Backend traits so new compositors or Windows can be added without rewriting the agent-facing API.

---

### 7. Architecture (High Level)

```
Agent (agentd / MCP host / shell)
        │
        ├─ MCP stdio (primary for ApexOS)
        ├─ CLI (subcommands + optional batch/JSON)
        └─ Optional warm daemon (Unix socket + token)
                │
                ▼
        apex-harness core (pure Rust)
                │
    ┌───────────┼───────────┬──────────────┐
    ▼           ▼           ▼              ▼
 A11yBackend  InputBackend  CaptureBackend  WindowBackend
 (AT-SPI /    (portal/      (portal /      (DE-specific
  atspi crate) libei /       xcap / grim /  + AT-SPI apps
               ydotool /     X11)           + EWMH/hyprctl/
               XTest)                       KWin/etc.)
```

**Recommended crates / tech (subject to final evaluation):**
- AT-SPI: `atspi` (+ zbus) — pure Rust, already used by serious a11y projects.
- Input: prefer XDG RemoteDesktop / libei portals on Wayland; ydotool / uinput as robust fallback; XTest / x11rb or enigo for pure X11.
- Capture: XDG Screenshot portal, then compositor-specific or `xcap` / grim.
- Window management: AT-SPI application roots + compositor-specific (GNOME Shell / KWin scripting / hyprctl / i3 IPC / EWMH) with clear priority and reporting.
- MCP: compatible with the rmcp / Apex protocol style already used by the project.
- Async runtime: tokio (matches Apex and atspi).

The public agent-facing surface stays stable; backends are swappable and report their capabilities.

---

### 8. Safety & Security Model

Because ApexOS agents can self-modify, control hardware, and persist memory, the harness must be conservative by default.

- **Sensitive-app denylist** (configurable): password managers, keyrings, banking-looking windows, etc. Block open/focus + mutations while they are frontmost or targeted unless explicitly overridden.
- **Policy integration:** Mutating tools carry annotations so Apex’s PolicyEngine can force human approval (especially for type/click sequences that look like send/post/pay/delete).
- **Audit log:** `~/.apex-harness/audit.jsonl` (or under Apex’s preferred state dir) for every mutation.
- **Daemon auth:** Owner-only Unix socket + short-lived or file-based token (0600).
- **No network exposure.**
- **Physical override:** Real user mouse/keyboard input is never blocked.
- **Least privilege:** Prefer portal-mediated input and capture on Wayland; document exact group / udev / capability needs for uinput.
- **Consent culture (skill + docs):** Agents must ask before outbound communications, financial actions, destructive file operations via GUI, or security-setting changes.

`doctor` surfaces the current safety posture and any elevated-permission state.

---

### 9. Platform Support & Roadmap

**Phase 1 – Linux MVP (priority)**
- X11 + Wayland detection.
- Strong support for GNOME, KDE/Plasma, Hyprland, i3/Sway (best-effort), generic EWMH/X11.
- AT-SPI tree + semantic actions working for well-behaved GTK/Qt apps.
- Reliable input path (portal or ydotool/uinput) and window-scoped screenshots.
- MCP + CLI + doctor/selftest.
- Integration path into ApexOS-RS workspace / plugins.toml.

**Phase 2 – Hardening & coverage**
- Better Electron / browser a11y handling.
- Presence indicator.
- Richer wait/stability primitives.
- Optional warm daemon as first-class.
- Packaging / install helpers that play nicely with Apex’s `install.sh`.

**Phase 3 – Windows**
- UI Automation backend + SendInput / modern input APIs.
- Same agent-facing tool surface.

**Later / optional**
- macOS parity (AX) if desired.
- Background / focus-stealing-free modes where the platform allows it.

---

### 10. Success Metrics / Acceptance

- `apex-harness doctor` produces a clear readiness report on a stock GNOME, KDE, and Hyprland session.
- `selftest` passes (visible mouse movement + safe interaction) when permissions are correct.
- An ApexOS agent can, without screenshots in the common case:
  - discover the frontmost app,
  - read a compact a11y tree / labels,
  - open a simple application,
  - click a named button or menu item via semantic action,
  - type into a text field,
  - take a scoped screenshot as fallback.
- Multi-step sequences via MCP or daemon stay responsive.
- Audit log and sensitive-app blocks function.
- Clean failure modes and capability reporting when a compositor or portal is missing.
- Fits ApexOS pure-Rust / self-update / policy model without special snowflake treatment.

---

### 11. Open Questions (for implementers / team)

- Exact crate/binary name and whether it lives inside the ApexOS-RS workspace from day one or starts as a sibling repo that is later vendored.
- How aggressively to cache AT-SPI proxies vs. always fresh snapshots.
- Whether a small expression / batch language is worth adding on top of discrete MCP tools (original desktop-harness used Python snippets; discrete tools are safer and more schema-friendly for MCP).
- Presence indicator implementation details (overlay window, compositor-specific, or simple cursor warping + notification).
- Exact sensitive-app heuristics and whether browser password fields can be reliably detected via a11y roles/states.
- Degree of self-evolution: should the agent be allowed (via Apex’s normal guarded update path) to improve the harness itself?

---

### Implementation Notes for the Team

- Start with `doctor` + AT-SPI snapshot + list windows/apps — these prove the environment and give immediate agent value.
- Keep the agent-facing tool schemas small and high-level; push compositor complexity into backends.
- Mirror the efficiency rules from the original skill in Apex documentation and any SKILL.md equivalent.
- Prefer progressive enhancement: a session that only has AT-SPI + ydotool should still be useful even if fancy window targeting is missing.

This PRD gives ApexOS-RS agents the missing “computer use” capability while staying true to the project’s pure-Rust, safety-conscious, offline-first, self-evolving character.

Ready for review, prioritization, and first vertical slice (doctor + AT-SPI snapshot + MCP skeleton).
