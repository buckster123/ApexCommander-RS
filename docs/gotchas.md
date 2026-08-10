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

- **Sensitive denylist is substring + local only.** It does not replace ApexOS PolicyEngine
  approval. Patterns live in defaults + optional `~/.config/apex-harness/sensitive.toml`.
