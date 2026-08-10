---
name: apex-desktop-harness
description: >-
  Linux desktop computer-use via AT-SPI (eyes + hands). Prefer shell and native
  tools first; use this harness for other apps' GUIs. MCP server apex-harness-mcp
  or CLI apex-harness.
---

# Apex Desktop Harness — agent skill

Give agents **real computer-use** on a Linux desktop without defaulting to
screenshot → vision → coordinate loops.

**Binary / MCP:** `apex-harness-mcp` (stdio) · CLI `apex-harness`  
**Ids:** `{bus}|{object_path}` (e.g. `:1.11|/org/a11y/atspi/accessible/968`)

## Efficiency rules (non-negotiable)

1. **Shell / native tools before GUI.** Files, git, package managers, APIs — use them.
2. **AT-SPI tree before screenshots.** Call `snapshot` / `find_elements`, not `screenshot`, in the common case.
3. **Element actions before coordinates.** `do_action` / `type_into` before `mouse_click` / `type_text`.
4. **Window-scoped capture before full display** when a screenshot is truly needed.
5. **No vision loop by default.** Screenshots are fallback for empty/custom canvases only.
6. **Ask before high-risk mutations.** Outbound messages, payments, deletes, credentials, security settings.
7. **Human physical input always wins.** Never fight the user for the pointer.
8. **`doctor` first** when the environment is unknown or a step fails with `Unavailable`.

## Recommended workflow

```
doctor
  → list_windows / frontmost
  → snapshot (compact tree) OR find_elements (role + name)
  → do_action / type_into
  → wait_for_element | wait_for_stable between steps if the UI is animating
  → screenshot only if the tree is empty or insufficient
```

### Tool cheatsheet

| Goal | Tool |
|------|------|
| Ready? | `doctor` · `selftest` |
| What's open? | `list_apps` · `list_windows` · `frontmost` |
| See structure | `snapshot` · `find_elements` · `focused_element` |
| Click / type (a11y) | `do_action` · `type_into` · `set_value` |
| Open an app | `launch` (`org.gnome.Calculator` or path) |
| Focus a window | `activate` |
| Wait | `wait` · `wait_for_element` · `wait_for_stable` |
| Fallback pixels | `screenshot` · `mouse_*` · `type_text` · `key` |

## Safety

- Mutating tools are annotated (`readOnlyHint` / `destructiveHint`) for PolicyEngine.
- Local **sensitive-app denylist** blocks password managers / keyrings by title/app name.
- Every mutation appends to `~/.local/share/apex-harness/audit.jsonl` (or `$APEX_HARNESS_STATE_DIR`).
- Prefer `selftest` without `confirm_mutate` in automation; enable mutate only with human consent.

## Failure recovery

| Error | Do this |
|-------|---------|
| `Unavailable` | Re-run `doctor`; install/fix AT-SPI, portals, or input tools |
| `NotFound` | Fresh `list_windows` / `snapshot`; ids die with app restarts |
| `Ambiguous` | Narrow `name` / `role`; re-snapshot |
| `PolicyBlocked` | Different target, or explicit override path + human approval |
| `do_action` false | Element may not support that action; try another action name or `snapshot` |

## Anti-patterns

- Screenshot every step “to be safe”
- Coordinate clicks when a named button exists in the tree
- Caching element ids across app restarts
- Typing passwords via `type_into` / `type_text` without explicit user request
- Launching shell pipelines as `launch` targets (metacharacters are rejected)

## Integration

Register MCP in ApexOS-RS `plugins.toml` — see `docs/apexos-integration.md`.  
Contract: `docs/design.md`. Product intent: `docs/PRD.md`.
