# Field matrix — compositor / DE coverage

S4 ledger. A row is **PASS** only when `apex-commander field-report` was run **inside that
live session** and returned `ok: true`. Host lacks a DE ⇒ row stays **PENDING** with
install/session notes — never fake a green cell.

## How to run

```sh
cd ~/Projects/ApexCommander-RS
cargo build -p apex-commander-cli
# from the graphical session under test:
./scripts/run-field-matrix.sh
# or:
cargo run -q -p apex-commander-cli -- field-report --markdown \
  > docs/field-evidence/<family>-<host>.json
# markdown section also on stderr — paste under the matching DE below
```

MCP: tool `field_report`.

## Summary table

| Family | Session type | Host evidence | Result | Notes |
|--------|--------------|---------------|--------|-------|
| **GNOME** (ubuntu:GNOME) | Wayland | Krackan laptop 2026-08-10 | **PASS** | See [evidence](field-evidence/gnome-ubuntu-wayland.md) |
| **KDE / Plasma** | — | *not installed as session on this host* | **PENDING** | Only `plasma-desktoptheme` package; no `plasma.desktop` session. Re-run script after installing Plasma Wayland/X11. |
| **Hyprland** | — | *not installed* | **PENDING** | No `hyprland` package / `hyprctl`. Re-run after Hyprland session exists. |
| Sway / i3 | — | not in S4 gate | n/a | Best-effort later; helpers probed (`swaymsg`/`i3-msg` absent here). |

## Expected backends by family

| Concern | GNOME Wayland | Plasma (expected) | Hyprland (expected) |
|---------|---------------|-------------------|---------------------|
| AT-SPI bus | `org.a11y.Bus` via session | same (at-spi2) | same if a11y enabled |
| Window list | AT-SPI app/frame roots | AT-SPI + optional KWin | AT-SPI + optional `hyprctl` |
| Activate / raise | GrabFocus often **false** / `NotSupported` | GrabFocus varies; KWin scripting later | GrabFocus weak; **hyprctl dispatch** later |
| Screenshot | **xdg-desktop-portal** (works here); Shell.Screenshot often AccessDenied | portal via xdg-desktop-portal-kde | **grim** (+ slurp) preferred; portal if configured |
| Input fallback | ydotool/wtype (not installed here) | same | ydotool/wtype common |
| Launch | `gtk-launch` / `gio` | same + `.desktop` | same |

## GNOME — verified (2026-08-10)

Host: Ubuntu 25.10 · `ubuntu:GNOME` · Wayland · family `gnome`.

Raw JSON: [`field-evidence/gnome-ubuntu-wayland.json`](field-evidence/gnome-ubuntu-wayland.json)

<!-- paste field-report --markdown below; kept in sync with last run -->

### gnome — ubuntu:GNOME (wayland)
- **Captured:** 2026-08-10T21:01:47.205323946+02:00
- **Result:** PASS
- **Apps / windows:** 13 / 20
- **Toolkits:** `{"clutter": 1, "gtk": 12}`
- **Checks:**
  - `doctor` **ok** — session=Wayland desktop=ubuntu:GNOME — AT-SPI hands+eyes ready; capture=ok input=no
  - `capture_probe` **ok** — backends: xdg-desktop-portal, gnome-shell-screenshot, gnome-screenshot-cli
  - `input_probe` **skip** — no ydotool/xdotool/wtype in PATH — element AT-SPI actions still work; install ydotool for coordinate fallback
  - `atspi_connect` **ok** — connected
  - `list_apps` **ok** — 13 apps; toolkits={"clutter": 1, "gtk": 12}
  - `list_windows` **ok** — 20 windows; focused_flag=2
  - `activate_grab_focus` **ok** — GrabFocus returned false on Some("untitled - Geany") — GrabFocus returned false — window may not accept focus
  - `snapshot` **ok** — role=frame nodes=60 truncated=true max_depth_hit=true
  - `find_button` **ok** — 3 button(s); first=Some("Minimize")
  - `screenshot` **ok** — backend=xdg-desktop-portal scope=window bytes=1464552 path=/home/andre/.local/share/apex-commander/screenshots/shot-20260810T190147.857.png
  - `compositor_helpers` **ok** — {"gio": true, "gnome-screenshot": true, "grim": false, "gtk-launch": true, "hyprctl": false, "i3-msg": false, "kdotool": false, "qdbus": false, "qdbus6": false, "slurp": false, "swaymsg": false, "wtype": false, "xdotool": false, "ydotool": false}
- **Summary:** field ok on gnome (ubuntu:GNOME) — apps=13 windows=20 toolkits={"clutter": 1, "gtk": 12}

### GNOME-specific gotchas (also in `gotchas.md`)

- Shell **"Main stage"** reports focused; `frontmost` deprioritizes it.
- **Shell.Screenshot** D-Bus often `AccessDenied`; portal path is the one that works.
- **GrabFocus** returns false or `NotSupported` on many GTK4/Wayland surfaces — do not treat as harness crash.
- Portal may also drop a copy under `~/Pictures`.

## Plasma — PENDING

### When you have a Plasma session

```sh
# install example (Ubuntu): plasma-workspace + plasma-desktop + xdg-desktop-portal-kde
# log into Plasma (Wayland preferred), then:
./scripts/run-field-matrix.sh
```

### What to watch

- Confirm AT-SPI apps list Qt toolkits (`qt` / `QTK` strings vary).
- Screenshot: portal vs Spectacle CLI.
- Whether GrabFocus works more often under KWin than GNOME.
- Optional later: KWin scripting / `qdbus` raise backends.

## Hyprland — PENDING

### When you have a Hyprland session

```sh
# ensure AT-SPI: often need `export GTK_MODULES=gail:atk-bridge` / toolkit a11y on
# install grim (and ydotool for input fallback)
./scripts/run-field-matrix.sh
```

### What to watch

- `hyprctl` present in helpers map.
- grim used when portal is missing or denied.
- Many electron apps may ship empty a11y trees — screenshot fallback path.
- Window raise almost certainly needs `hyprctl dispatch focuswindow` (post-v1 backend).

## Protocol for updating this file

1. Run `field-report --markdown` in the target session.
2. Copy JSON to `docs/field-evidence/<family>-<tag>.json`.
3. Replace the DE section above with the markdown section.
4. Flip the summary table cell to **PASS** and date it.
5. Add any new invariant to `gotchas.md` in the same commit.

**Don't** mark Plasma/Hyprland PASS from a GNOME session or from package presence alone.
