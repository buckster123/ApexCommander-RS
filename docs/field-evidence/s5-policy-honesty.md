# S5 live — policy honesty (2026-08-16)

Host: ubuntu:GNOME Wayland (same laptop as the S4 GNOME row). Binary: workspace
`apex-commander-cli` on `main` @ `10264fd`. No password-manager window was
open; vault **launch** used the default denylist, window **snapshot/do_action**
used a throwaway `sensitive.toml` (`deny=geany`) so we did not open a vault
or write the user config.

| Check | Result | Evidence |
|-------|--------|----------|
| `doctor` policy + audit caps | **PASS** | `policy` available, 13 patterns, `allow_override=false`; `audit` writable at `~/.local/share/apex-commander/audit.jsonl` |
| Control snapshot (default policy) | **PASS** | `snapshot --name geany` → `role=frame` nodes=9 |
| Default denylist on launch | **PASS** | `launch bitwarden` and `launch 1password` → exit **5** `PolicyBlocked` (patterns `bitwarden` / `1password`) |
| Window-title guard (not element id) | **PASS** | With `deny=geany`: `snapshot --name geany` and `do-action --id :1.11\|/…/882` (Minimize) → exit **5** `PolicyBlocked` haystack `geany *untitled - geany` |
| `allow_override` audited | **PASS** | Same deny + `allow_override=true`: snapshot succeeded; audit line `tool=policy_override` pattern `geany` (isolated `APEX_COMMANDER_STATE_DIR`) |
| `type_text` does not store payload | **PASS** (honest degrade) | No ydotool/wtype/xdotool. `type-text CANARY_S5_TYPE_TEXT_PROBE` → exit **2** `Unavailable` (no type backend). Isolated `audit.jsonl` created empty; canary **absent**. Success-path detail covered by unit test `typed_detail_has_no_secrets`. |

No desktop mutation landed on the Geany window (blocked before DoAction).
Temp config/state dirs were removed after the run.
