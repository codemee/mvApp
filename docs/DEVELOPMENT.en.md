**English** | [繁體中文](DEVELOPMENT.md)

# mvApp development and technical notes

For general usage see the [README](../README.en.md). This document is for developers who want to build, modify or understand how mvApp works internally.

## Environment and build

- Platform: Windows only (target `x86_64-pc-windows-msvc`).
- Tools: Rust stable (edition 2024) and the Visual Studio Build Tools (including the Windows SDK; a resource compiler is needed to compile `app.rc`).
- Main dependencies: `eframe` / `egui` 0.37 (UI), `windows` 0.62 (Win32 API), `tray-icon` 0.26 (system tray), `raw-window-handle` 0.6, and `embed-resource` (embeds resources at build time).

| Command | Purpose |
|---|---|
| `cargo run` | Run in debug mode (with a console, handy for output) |
| `cargo run --release` | Run in release mode (Windows subsystem, no console) |
| `cargo build --release` | Build the release binary at `target\release\mvapp.exe` (about 10 MB) |
| `cargo test` | Run the tests (see "Tests") |

### Release build settings

- `.cargo/config.toml` sets `-C target-feature=+crt-static`, which **statically links the VC++ runtime**, so the exe runs on machines without the VC++ Redistributable.
- `[profile.release]` in `Cargo.toml` uses `lto = true`, `codegen-units = 1`, `strip = true` and `panic = "abort"` to shrink the binary.
- `build.rs` compiles `app.rc` via `embed-resource`, which contains two resources:
  - Resource 1: `app.manifest`, declaring PerMonitorV2 DPI awareness.
  - Resource 2: `assets/icon.ico` (multi-size, 16 to 256, each size drawn natively). The tray icon loads it by ordinal 2 so Windows can pick the right small size.

## Project structure

| File | Responsibility |
|---|---|
| `src/main.rs` | Entry point: single-instance check, graphics backend setup, CJK font loading, window creation, recording the main window HWND |
| `src/app.rs` | UI: toolbar, monitor panes, drag and drop, status bar, palette and styles, startup and close window behavior |
| `src/monitors.rs` | Monitor enumeration (position, work area, DPI) |
| `src/windows_list.rs` | Window enumeration and filtering |
| `src/icons.rs` | Fetching window icons and turning them into egui textures (with a cache) |
| `src/mover.rs` | Coordinate mapping and Win32 calls for moving a window |
| `src/toolbar.rs` | Toolbar icon buttons and the icons drawn with a painter |
| `src/tray.rs` | Tray icon, right-click menu, showing/hiding the main window |
| `src/single_instance.rs` | Single instance (named mutex / event) |
| `src/autostart.rs` | Run at startup (registry) |
| `src/settings.rs` | Reading and writing the settings file |
| `src/i18n.rs` | UI string translation and system language detection |
| `assets/` | Program icon (`icon.png`, `icon.ico`) |

## How it works

### Monitor and window enumeration

- Monitors: `EnumDisplayMonitors` + `GetMonitorInfoW` give the full rectangle, work area and primary flag, and `GetDpiForMonitor` gives the DPI. They are sorted by left edge, then top edge, so the pane order matches the physical layout.
- Windows: after `EnumWindows`, the following are excluded:
  - Invisible windows and windows that have an owner.
  - Windows with `WS_EX_TOOLWINDOW` or `WS_EX_NOACTIVATE` and without `WS_EX_APPWINDOW`.
  - Windows "cloaked" by DWM (for example UWP windows on other virtual desktops).
  - Windows without a title, the desktop shell windows (`Progman`, `WorkerW`), and mvApp's own window.
- The monitor a window is on comes from `MonitorFromWindow(..., MONITOR_DEFAULTTONEAREST)`.
- The UI re-enumerates every 600 ms, and pauses while a drag is in progress so the list doesn't jitter.

### Icon retrieval (`icons.rs`)

It tries, in order:

1. **UWP / packaged apps** (only for windows hosted by `ApplicationFrameHost.exe`): read the window's AppUserModelID with `SHGetPropertyStoreForWindow`, then ask the shell for the image of `shell:AppsFolder\<AUMID>` (`IShellItemImageFactory`). Otherwise you would get the generic icon of the host process.
2. The window's icon: `WM_GETICON` (big, then small), then the class icon `GCLP_HICON`.
3. The executable's icon: `SHGetFileInfoW`.

When converting an `HICON` to RGBA, old icons without an alpha channel get their alpha from the mask, and the premultiplied-alpha bitmaps returned by the shell are un-premultiplied. The result is cached as a texture keyed by HWND and removed only when the window disappears.

### Moving a window (`mover.rs`)

- Coordinate mapping `map_rect`: the window's position and size are scaled by the ratio "source monitor work area -> target monitor work area" and then clamped into the target work area. It is a pure function with unit tests.
- Flow:
  - Normal window: `GetWindowRect` -> map -> `SetWindowPos`.
  - Maximized window: restore, move, then maximize again.
  - Minimized window: restore, move, then minimize again (so a window that was maximized and then minimized ends up in its normal state).
- Between monitors with different DPI, the system may resize the window after the first move, so `SetWindowPos` is called twice.
- Failures (most commonly the target window runs as administrator and UIPI blocks the move) are returned as a `MoveError`; the UI translates the message for the current language and the program never crashes.

### UI and drag and drop (`app.rs`)

- It uses egui's `dnd_drag_source` / `dnd_drop_zone`; the drag payload is the window HWND. On release, a move is only issued if the target monitor differs from the one the window is on.
- Layout: when there is enough width (at least 300 px per pane) the panes sit side by side with equal width; otherwise they stack vertically and the whole area scrolls.
- Note: during a drag, `dnd_drop_zone` repaints the drop zone with the `widgets.inactive` / `widgets.active` styles, so `apply_styles` deliberately sets those two to "same as the pane when idle, accent-colored outline when droppable"; otherwise the drop feedback would be wrong.
- The status bar message is stored as data (`Status`) rather than text, so it can be re-translated immediately when the language changes.

### System tray and window lifecycle (`tray.rs`, `app.rs`)

- It starts in the tray only. But eframe unconditionally calls `set_visible(true)` after the first frame, so `with_visible(false)` is not enough. Instead:
  1. Right after the window is created it is made `WS_EX_LAYERED` with alpha 0 (fully transparent).
  2. On the second frame it is hidden with Win32 `ShowWindow(SW_HIDE)` and made opaque again.
  3. This way the user never sees the window flash.
- Showing and hiding use Win32 directly on the main window HWND rather than egui `ViewportCommand`s, because tray events can fire outside egui's update loop, and a hidden window is not repainted.
- Close button: `close_requested` is intercepted, `CancelClose` is sent and the window is hidden.
- The right-click menu "Exit" sets a flag and posts `WM_CLOSE` to the main window to follow the normal close path; if that is not handled while the window is hidden, the process is forcibly ended after 1.5 seconds.
- If creating the tray icon fails, the main window is shown directly and closing it quits the program, so users can't lose the app.

### Single instance (`single_instance.rs`)

- Very early at startup it creates the named mutex `Local\mvApp.SingleInstance`; if it already exists (`ERROR_ALREADY_EXISTS`), another instance is running.
- The first instance also creates an auto-reset named event `Local\mvApp.ShowWindow` and waits on it in a background thread. A second instance just calls `SetEvent` and exits; the first instance then shows its main window.
- The `Local\` prefix limits this to the current login session, so different users can each run one.

### Run at startup (`autostart.rs`)

- It adds a value named `mvApp` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, containing the quoted full path of the current executable. It only writes to the current user's hive and needs no administrator rights.
- If, at startup, it is already enabled but the path differs from the current executable (for example the file was moved), the path is updated.
- The tray menu's check mark reflects the actual registry contents.

### Settings and localization (`settings.rs`, `i18n.rs`)

- The settings file is `%APPDATA%\mvapp\settings.txt`, two `key=value` lines:

  ```
  theme=system|light|dark
  lang=system|zh|en
  ```

  It is written only when the user changes something; write failures are silently ignored (settings are not critical).
- When the language preference is "follow system", `GetUserDefaultLocaleName` decides: a locale starting with `zh` means the Traditional Chinese UI, anything else means English.
- All UI strings live in `Tr` in `i18n.rs`; messages such as move failures travel as an enum (`MoveError`) and are only translated when displayed.
- CJK fonts: at startup it tries to load `msjh.ttc`, `msyh.ttc` and `meiryo.ttc` from `C:\Windows\Fonts` as fallback fonts; otherwise window titles would render as boxes.

### Palette and styling

- The look follows Windows 11 Fluent: a neutral background, thin-bordered groups (title set into the border), a white list area inside, and gray thin-line icons.
- Colors are defined in `Palette` in `app.rs`: the light theme takes its colors from the Gundam RX-78-2 (blue, red, yellow, gray) and the dark theme from Neon Genesis Evangelion Unit-01 (dark purple, neon green, orange). Large areas use neutral colors; the vivid colors are only used for the per-monitor icons and the drop highlight.
- The egui style is configured once for light and once for dark; egui applies the right one when the theme changes.
- Toolbar spec: height 42, padding 4 on all sides, buttons 34x34, icon canvas 20x20 centered, 4 px between buttons; regular buttons have radius 4 and state buttons radius 8; the separator is 1 px wide with 5 px vertical and 4 px horizontal margins. All toolbar icons are drawn with a painter (`toolbar.rs`), with no image files.

### Graphics backend

On some Intel drivers the Vulkan component (`igvk64.dll`) crashes the program at startup when wgpu uses Vulkan by default. So `main.rs` defaults `WGPU_BACKEND` to `dx12`; if the user has set that environment variable themselves, their value is respected.

## Tests

```powershell
cargo test
```

- `mover`: unit tests for coordinate mapping (same-size monitors, different-size monitors, clamping).
- `smoke`: smoke tests for monitor, window and icon enumeration. They need to run inside a logged-in desktop session.
- `autostart`: actually writes and then deletes the `Run` value under `HKCU`. If the user already has run-at-startup enabled when the test runs, it is skipped so the real setting is never touched.

## Release process

Releases are currently made **locally**; no CI is set up:

1. Bump the version in `Cargo.toml`.
2. `cargo build --release`, and check that the exe starts correctly and references no dynamic VC++ libraries.
3. Commit and push to `main`.
4. Compute the exe's SHA-256 and put it in the release notes (bilingual).
5. Create the release and upload the exe with `gh release create vX.Y.Z target\release\mvapp.exe --notes-file ...`.
6. Download it again from the public URL and compare the SHA-256.

## Program icon

`assets/icon.png` and `assets/icon.ico` were drawn with PowerShell and `System.Drawing`: every size (16, 20, 24, 32, 40, 48, 64, 128, 256) is drawn natively, with thicker lines at small sizes so they don't break up when shrunk. The arrow sits in the lower half of the window's content area (below the title bar), away from the title bar, to keep the picture simple. The overall vertical position was tuned by measurement (bounding-box center about 49.6%, pixel-mass centroid about 50.5%, both within +/-1% of the canvas center), so the icon looks vertically centered on the taskbar; centering the bounding box alone is not enough, you have to look at the pixel-mass centroid too or the icon looks too high or too low. The script that generates the icon is not kept in version control.

## License

GPL-3.0-or-later; see [LICENSE](../LICENSE) for the full text.
