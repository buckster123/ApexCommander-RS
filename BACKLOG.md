# ApexCommander-RS backlog — slice ledger

A row gets its ✅ when the slice is **merged, deployed, and verified live** — not when tests
pass (house doctrine #5). Notes carry the date and the evidence.

## v1 (agent hands on Linux)

- [x] **S0 — bootstrap** (2026-08-10): Launchpad stamp, crate rename to `apex-commander`,
  CLI + MCP faces, `docs/{CHARTER,design,PRD}.md`, CI, MIT licence, env-only `doctor`,
  unit tests for types/report JSON. Evidence: `cargo test --workspace` green;
  `apex-commander doctor` / MCP `tools/list` + `doctor` call.
- [x] **S1 — eyes (AT-SPI)** (2026-08-10): connect bus; `list_apps` / `list_windows` /
  `frontmost` / `activate` (GrabFocus); compact `snapshot` + `find_elements` +
  `focused_element`. Pure find/id unit tests (12 total). Field evidence on ubuntu:GNOME
  Wayland: doctor reports `atspi` bus live · 11 apps; geany snapshot shows frame/menu/tool
  bar; `find --name geany --role button` returns Minimize/Restore/Close/New with bounds +
  actions. Note: GrabFocus often false on Wayland (gotcha); compositor raise deferred to S2.
- [x] **S2 — hands + capture** (2026-08-10): AT-SPI `do_action` / `type_into` / `set_value`;
  input fallbacks (`ydotool`/`xdotool`/`wtype` when installed); portal screenshot + window
  crop via a11y bounds; audit JSONL (`~/.local/share/apex-commander/audit.jsonl`); sensitive
  denylist stub (`policy`). Field: portal shot ok; `do_action Click` on Geany “New” → true
  + audit line; doctor reports capture=ok input=no (no ydotool). 19 unit tests.
- [x] **S3 — agent fit** (2026-08-10): `wait` / `wait_for_element` / `wait_for_stable`;
  `launch`; `selftest` (mutate opt-in); skill `skills/apex-commander/SKILL.md`;
  `docs/apexos-integration.md` (plugins.toml template). Field: selftest ok on GNOME;
  launch org.gnome.Calculator; wait_for_element finds button. PRD §10 happy path covered
  without screenshots (discover → snapshot/find → do_action).
- [x] **S4 — field** (2026-08-10): `field-report` / MCP `field_report` +
  `scripts/run-field-matrix.sh` + `docs/field-matrix.md` ledger.
  **GNOME Wayland PASS** (ubuntu:GNOME, 13 apps / 20 windows, portal screenshot, honest
  GrabFocus NotSupported). Plasma + Hyprland **PENDING** (sessions not installed on host —
  re-run script after login). Compositor gotchas in `docs/gotchas.md`. Evidence under
  `docs/field-evidence/`.
- [ ] **S5 — policy honesty** (2026-08-15): denylist on window title/app (not element ids);
  fail-closed unclassified; audited `allow_override`; no secrets in audit detail;
  `Protected` redaction; MCP process-local AT-SPI session; honest `field_report` /
  GrabFocus `NotSupported`; `click_element` on MCP. Findings: `docs/audit-2026-08-15.md`.
  ✅ when merged + live: `do_action` on a vault window → `PolicyBlocked`; `type_text`
  audit line has no payload text; `doctor` shows `policy` + `audit` caps.

## Post-v1 parking

- Optional warm daemon (owner-only socket + token, proxy cache)
- Presence indicator (agent owns the pointer)
- Electron / browser a11y hardening
- Packaging / install helpers aligned with Apex `install.sh`
- Windows UI Automation + SendInput backends
- macOS AX parity (only if desired)
- Batch / multi-step expression language (only if discrete tools prove insufficient)
- Repo display-name shortening (`ApexHarness-RS` etc.) without renaming the crate
