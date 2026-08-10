# ApexDesktopHarness-RS — the contract

> **Contract first** (house doctrine #1). This document is pinned **before** the code it
> describes. Code follows this doc; a PR that changes behaviour updates this doc in the same
> commit. When the two disagree, that is a bug in one of them — find out which, don't guess.
>
> Product intent and roadmap live in [`PRD.md`](PRD.md). This file is the **load-bearing
> wire/API contract** implementers code against.

## Scope

**Covers**

- Agent-facing tool surface (MCP + CLI) for Linux desktop control.
- Shared types and mutation annotations used by PolicyEngine / hosts.
- Backend capability reporting (`doctor`) and honest degrades.
- Audit log shape for mutations.
- Environment knobs and state locations.

**Does not cover**

- Vision-model / screenshot→coordinate loops as the default path (explicit fallback only).
- Remote desktop, multi-user isolation, or network-facing control planes.
- Replacing shell, git, filesystem, or HTTP tools.
- Windows / macOS backends (trait-shaped for later; not v1).
- ApexOS-RS `plugins.toml` registration (consumer-side; documented as integration notes).

## Shape

```
Agent (agentd / MCP host / shell)
        │
        ├─ MCP stdio  (apex-harness-mcp)     ← primary for ApexOS-RS
        ├─ CLI        (apex-harness)         ← humans + scripts
        └─ Optional warm daemon (post-v1)    ← Unix socket + token
                │
                ▼
        apex-harness core
                │
    ┌───────────┼───────────┬──────────────┐
    ▼           ▼           ▼              ▼
 A11yBackend  InputBackend  CaptureBackend  WindowBackend
```

Core owns all logic. Faces are thin adapters. Backends are swappable traits that report
capabilities; the public tool surface stays stable across compositors.

## The wire / API surface

### MCP

- Protocol: `2024-11-05`, hand-rolled newline-delimited JSON-RPC over stdio. **No SDK.**
- stdout is JSON-RPC only; all logging → stderr.
- Tool failures → MCP `isError` **results** with helpful text. JSON-RPC errors are for
  protocol breakage only.
- Mutating tools carry annotations (`readOnlyHint` / `destructiveHint`) so hosts can require
  approval.

### Tool catalog (v1 target)

| Tool | Purpose | Mutation | Status |
|---|---|---|---|
| `doctor` | Structured readiness (session, AT-SPI, input, capture, window backend, recommendations) | read-only | **live** |
| `list_apps` | AT-SPI application roots (name, pid, toolkit, window_count, id) | read-only | **S1 live** |
| `list_windows` | Top-level frames/dialogs (title, app, pid, focused, bounds, id) | read-only | **S1 live** |
| `frontmost` | Best-effort focused window (shell chrome deprioritized) | read-only | **S1 live** |
| `activate` | Focus / raise via AT-SPI `GrabFocus` (by id / name / pid / frontmost) | mutating | **S1 live** |
| `launch` | Open app by desktop entry or executable (safety-checked) | mutating | planned S2 |
| `snapshot` | Compact a11y tree (max_depth + max_nodes; bounds + actions) | read-only | **S1 live** |
| `find_elements` | Semantic selectors (role + name/text/state) under a target | read-only | **S1 live** |
| `focused_element` | Details of the currently focused a11y node under a target | read-only | **S1 live** |
| `do_action` / `click_element` | AT-SPI action by name/index (default Click) | mutating | **S2 live** |
| `type_into` | EditableText set/append | mutating | **S2 live** |
| `set_value` | Value interface (slider/spin) | mutating | **S2 live** |
| `mouse_move` / `mouse_click` / `type_text` / `key` | Real input via ydotool/xdotool/wtype when installed | mutating | **S2 live** (probe) |
| `screenshot` | Display or window-crop → path under state dir | read-only* | **S2 live** |
| `wait` / `wait_for_element` | Stability / presence helpers | read-only | planned S3 |
| `selftest` | Non-destructive smoke (wiggle + type needs confirm) | mutating | planned S3 |

\*Screenshot is read-only w.r.t. the desktop but may write a file under the state dir.

CLI mirrors the same verbs as subcommands (`apex-harness doctor`, `apex-harness snapshot …`)
with `--json` for machine-readable stdout.

### Result shapes (shared)

All tool results that faces render as text should also be serializable as JSON matching the
types below. MCP tools return `content: [{ type: "text", text: <json or prose> }]` in S0;
later slices may add structured `structuredContent` when hosts support it.

#### `DoctorReport`

```json
{
  "ok": true,
  "session": "wayland",
  "desktop": "Hyprland",
  "capabilities": [
    { "name": "atspi", "available": true, "detail": "bus live" }
  ],
  "recommendations": ["…"],
  "summary": "…"
}
```

`ok` means “safe to attempt GUI work with the available backends,” not “every backend is
perfect.” Missing backends are listed with `available: false` and a real reason.

#### Errors

Faces map `HarnessError` variants:

| Variant | Meaning | Agent recovery |
|---|---|---|
| `Unavailable` | Backend/capability missing | Read `doctor`; degrade or install |
| `PolicyBlocked` | Sensitive app / approval required | Ask human; do not retry blindly |
| `NotFound` | Window/element/app missing | Fresh `snapshot` / `list_windows` |
| `Ambiguous` | Multiple matches | Narrow selector; re-snapshot |
| `Other` / `Io` | Real reason string | Surface to user; do not invent success |

## Types

Core shared types live in `apex_harness::types` and are **serde load-bearing**:

| Type | Role |
|---|---|
| `MutationClass` | `read_only` \| `mutating` \| `destructive` |
| `SessionKind` | `x11` \| `wayland` \| `unknown` |
| `Capability` | `{ name, available, detail? }` |
| `AppInfo` | Application root row |
| `WindowInfo` | Discovery row (`id`, title, app, pid, focused, bounds, role) |
| `Bounds` | `{ x, y, width, height }` physical pixels, origin top-left |
| `A11yNode` | Compact tree node (`id?`, role, name, description, value, states, actions, bounds, children) |
| `SnapshotOpts` | `max_depth` (default 6), `max_nodes` (default 200), bounds/actions flags |
| `FindQuery` | role / name / text / state filters + `max_results` |
| `ElementHit` | Find result with breadcrumb `path` |
| `TargetRef` | `id` \| `name` \| `pid` \| `frontmost` |
| `ActivateResult` | `{ ok, id, title?, detail }` |
| `ActionResult` | `{ ok, id, action?, detail }` for do_action / type_into / set_value |
| `ScreenshotResult` | `{ path, scope, backend, bytes, bounds? }` |
| `InputBackendKind` | `ydotool` \| `xdotool` \| `wtype` \| `none` |

**Ids:** `{bus_unique_name}|{object_path}` (e.g. `:1.11|/org/a11y/atspi/accessible/943`).

Serialization: `snake_case` enums; omit empty optionals / empty vectors where annotated.

## Lifecycle / state machine

### Cold call (CLI or MCP spawn)

```
spawn → (optional warm connect, post-v1) → tool call → backend probe if needed → result
```

No long-lived state required in v1. Each process may open AT-SPI / input devices on demand.

### Optional daemon (post-v1)

```
start → auth (owner-only socket + token) → cache proxies → serve calls → stop
```

- Socket under XDG runtime dir, mode `0600`.
- Token file mode `0600`.
- No network listeners. Ever.

### Mutation audit

Every mutating tool appends one JSONL line to the audit log:

```json
{
  "ts": "RFC3339",
  "tool": "click_element",
  "mutation": "mutating",
  "target": { "window": "…", "role": "button", "name": "OK" },
  "result": "ok|error",
  "detail": "…"
}
```

Path: `$APEX_HARNESS_STATE_DIR/audit.jsonl` (default `~/.local/share/apex-harness/audit.jsonl`).

## Environment

| Var | Default | Purpose |
|-----|---------|---------|
| `APEX_HARNESS_STATE_DIR` | `~/.local/share/apex-harness` | Audit log, tokens, screenshot drops |
| `APEX_HARNESS_CONFIG_DIR` | `~/.config/apex-harness` | Sensitive-app list, policy overlays |
| `APEX_HARNESS_LOG` | `info` (faces may default `warn` for CLI) | `tracing` filter via env (also `RUST_LOG`) |
| `DISPLAY` / `WAYLAND_DISPLAY` | session | Session detection |
| `XDG_CURRENT_DESKTOP` | session | Desktop hint for `doctor` |

Config files win over env for persistent knobs after first write (when config lands). Secrets
never go in the repo; token files are `0600`.

## Invariants

1. **AT-SPI before pixels.** Tools that can act via the accessibility tree must not default to
   coordinate clicks or screenshots.
2. **Shell before GUI.** Skill/docs teach agents to prefer shell/native tools when they exist.
3. **Element actions before coordinates.** `do_action` / value interfaces beat `mouse_click`.
4. **Window-scoped capture before full-display.**
5. **Human physical input always wins.** Never grab exclusive input in a way that blocks the user.
6. **No fake success.** Missing portal, bus, or permission → stated degrade with the real reason.
7. **stdout sacred on MCP.** No `println!` for logs.
8. **Audit every mutation.** No silent desktop side effects.
9. **Sensitive-app denylist.** Password managers / keyrings / banking-looking surfaces block
   open/focus/mutate unless explicitly overridden (override is itself audited).
10. **No network listeners by default.** Daemon is owner-only Unix socket only.
11. **Pure-fn test surface.** Selectors, tree compactors, policy matchers, report builders are
    unit-tested; D-Bus / portal I/O is thin glue with optional field tests.

## Honest degrades

| Condition | Behaviour |
|---|---|
| No `DISPLAY` / `WAYLAND_DISPLAY` | `doctor.ok = false`, session `unknown`, clear recommendation |
| AT-SPI bus missing | Capability `atspi.available = false`; snapshot/find tools return `Unavailable` |
| Input backend missing | Element AT-SPI actions may still work; coordinate tools return `Unavailable` |
| Capture portal denied | `screenshot` returns `Unavailable` with portal/compositor detail |
| Sensitive app frontmost | Mutating tools → `PolicyBlocked` |
| Multiple element matches | `Ambiguous` with count + short names — agent re-snapshots |
| Tool not yet implemented (slice gap) | MCP `isError` text naming the tool and pointing at `BACKLOG.md` |

## Integration notes (ApexOS-RS)

Preferred registration: MCP plugin stdio entry in `plugins.toml` pointing at the release
binary `apex-harness-mcp`. High-risk tools route through existing PolicyEngine / approval UI.
Screenshots and audit events may be ingested into Cerebro by the host — the harness does not
embed Cerebro.

Standalone is first-class: any MCP host or shell script can use the same surface with zero
ApexOS dependency.

## Open questions

Carried from the PRD; resolve with a charter amendment when decided:

1. How aggressively to cache AT-SPI proxies vs always-fresh snapshots.
2. Whether a small batch/expression language is worth it (lean: discrete tools first).
3. Presence-indicator implementation (overlay vs compositor-specific vs none in v1).
4. Sensitive-app heuristics depth (title/class lists vs a11y password-field detection).
5. Exact repo/crate name longevity (`ApexDesktopHarness-RS` is the folder; binary/crate is
   `apex-harness` — rename of the git remote/title is allowed later without breaking the crate).
