# Gotchas — the invariant ledger

> **RULE: before modifying ANY subsystem, grep this file for it and read the matching
> entries.** These are load-bearing invariants — most were written after something broke
> on a live node, and many end with an explicit "don't do X" that a future change would
> otherwise walk straight into.
>
> **A newly discovered gotcha goes HERE**, not in CLAUDE.md. Docs travel with code —
> update this file in the same PR as the change that discovered or altered an invariant.
>
> Format: one bullet, **bold lead naming the invariant**, then the story, ending with the
> explicit don't. Cross-project version drift lives in
> `~/Projects/Launchpad-RS/docs/sharp-edges.md` instead.

- **Element ids are `{bus}|{path}` on the a11y bus, not X11/Wayland surface ids.** Bus unique
  names like `:1.11` restart with the app. Don't cache ids across process lifetimes; re-list
  after relaunch.

- **Recursive `async fn` is illegal without boxing.** Tree walks must be iterative (arena /
  stack) or `Box::pin`. Don't reintroduce recursive `walk_node`/`focused_dfs` "for clarity".

- **GNOME Shell "Main stage" often owns AT-SPI focus.** `frontmost` deprioritizes shell chrome
  (`gnome-shell`, `gjs` Desktop Icons). Don't treat "Main stage" as a useful agent target without
  an explicit id.

- **`Component.GrabFocus` frequently returns false on Wayland.** Raising/focusing real windows
  may need a compositor backend (hyprctl / gdbus Shell / KWin) in S2+. Don't claim activate
  succeeded when GrabFocus is false — surface the boolean honestly.

- **Headless CI has no AT-SPI bus.** Unit tests cover pure find/id/types only. Live probes are
  field tests; `doctor` must degrade with a real reason, never panic. Don't add integration
  tests that fail the default `cargo test` without a display.

- **MCP stdout is sacred.** All tracing → stderr. A stray `println!` in the MCP face corrupts
  JSON-RPC.

- **Portal screenshots land a copy under the state dir.** The portal may also leave a file in
  `~/Pictures`. Don't assume only one path exists; always use the path returned in
  `ScreenshotResult`.

- **Window crop uses AT-SPI screen extents.** Maximized windows may report full-display bounds
  — crop then equals full capture. Don't assume crop implies a small ROI.

- **Coordinate input is optional.** Without `ydotool`/`xdotool`/`wtype`, mouse/type_text tools
  return `Unavailable`. Prefer `do_action` / `type_into`. Don't hard-fail `doctor.ok` on missing
  input tools.

- **Sensitive denylist matches title + app name, never element ids.** `{bus}|{path}` will
  not contain `bitwarden`. Classify via bus-matched windows (or frontmost for coordinate /
  full screenshot). Don't call `guard_name(element_id)` and think vaults are blocked.

- **Fail closed when unclassified.** If AT-SPI cannot name the frontmost/target window,
  refuse the mutation rather than injecting blindly. Don't "degrade open" on policy.

- **`allow_override` is audited.** A `policy_override` JSONL line is written before the
  tool proceeds. If the audit log is not writable, the override is refused.

- **Typed text never goes in audit detail.** `type_text` / `type_into` record a character
  count only. Don't format `run_cmd` argv (which includes the secret) into `detail`.

- **`Protected` values are `[redacted]`.** Password-role text must not appear in snapshots.
  Don't re-read `Text` after seeing that state.

- **`field_report` is not read-only.** It may GrabFocus and write a PNG. Don't advertise
  `readOnlyHint: true` for it.

- **Sensitive denylist is substring + local only.** It does not replace ApexOS PolicyEngine
  approval. Patterns live in defaults + optional `~/.config/apex-commander/sensitive.toml`.

## Compositor / DE matrix (S4)

Full ledger: [`field-matrix.md`](field-matrix.md). Re-run `scripts/run-field-matrix.sh` inside
each live session; never mark Plasma/Hyprland from a GNOME session.

### GNOME (Wayland) — verified

- **Portal screenshots work; Shell.Screenshot often AccessDenied.** Prefer portal path; don't
  treat AccessDenied as total capture failure if portal is available.
- **GrabFocus may return D-Bus `NotSupported`** (e.g. Ptyxis frames), not only `false`. Map both
  to honest activate detail — don't panic or claim focus moved.
- **focused_flag counts include shell chrome.** Always filter Main stage / gjs Desktop Icons for
  agent targets.
- **Session file on Ubuntu is often only `ubuntu.desktop`.** Family classification uses
  `XDG_CURRENT_DESKTOP=ubuntu:GNOME` → `gnome`.

### Plasma (KDE) — pending live run

- Expect **xdg-desktop-portal-kde** for capture; optional Spectacle CLI later.
- Qt toolkit strings in AT-SPI may differ (`qt`, `Qt`, …) — histogram is informational.
- Window raise may need **KWin** scripting/`qdbus` when GrabFocus is weak (post-v1 backend).
- Don't assume GNOME Shell.Screenshot exists.

### Hyprland — pending live run

- Prefer **grim** (+ slurp for regions) when portal is unset; probe helpers map includes `hyprctl`.
- Enable toolkit a11y (GTK a11y modules) or AT-SPI trees stay empty for many apps.
- Window focus/raise almost always needs **`hyprctl dispatch`** — AT-SPI GrabFocus alone is not
  enough; don't claim activate success without a compositor backend.

### Cross-DE

- **field-report PASS ≠ GrabFocus true.** PASS means checks completed with honest degrades.
- **Only one Wayland session file on this host** (`ubuntu.desktop`) — Plasma/Hyprland need install
  + login before their matrix rows can flip green.
