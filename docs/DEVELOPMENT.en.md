**English** | [繁體中文](DEVELOPMENT.md)

# mvApp development and technical notes

For general usage see the [README](../README.en.md). This document is for developers who want to build, modify or understand how mvApp works internally.

## Environment and build

- Platform: Windows only (target `x86_64-pc-windows-msvc`).
- Tools: Rust stable (edition 2024) and the Visual Studio Build Tools (including the Windows SDK; the resource compiler `rc.exe` is needed at build time).
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
- `build.rs` generates a resource file in `OUT_DIR` at build time and compiles it into the exe via `embed-resource`. It contains three things:
  - Resource 1: `app.manifest`, declaring PerMonitorV2 DPI awareness.
  - Resource 2: `assets/icon.ico` (multi-size, 16 to 256, each size drawn natively). The tray icon loads it by ordinal 2 so Windows can pick the right small size.
  - Version information (`VERSIONINFO`): the display name and version number; see "Display name and version" below.
  - If the resource compilation fails, the build fails, so the icon or version information can't silently go missing.

### Display name and version

- The display name is "mvApp-應用程式歸位器" followed by the version number (for example `v0.1.2`). The version comes from `version` in `Cargo.toml`, so **bumping a release only touches that one place**.
- The display name is used in two kinds of places:
  - The window title and tray tooltip: `Tr::app_title()` in `src/i18n.rs`. The Traditional Chinese UI shows "mvApp-應用程式歸位器 v<version>" and the English UI shows "mvApp - Move windows between monitors v<version>".
  - The exe's version information (`FileDescription`, `ProductName`, `FileVersion`, `ProductVersion`), written by `build.rs`. **This is what the taskbar hover tooltip, Task Manager and the file properties "Details" tab show**; without version information Windows falls back to the file name `mvapp`.
- The name string exists once in `src/i18n.rs` (`APP_NAME`) and once in `build.rs` (`APP_NAME`); change both when renaming.
- The following are internal identifiers, not display names, and must **not** be changed when renaming, or existing settings and the single-instance mechanism would break: the run-at-startup registry value name `mvApp`, the mutex / event names, the settings folder `%APPDATA%\mvapp`, and the exe file name `mvapp.exe`.

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
| `src/hotkey.rs` | Global hotkey: parsing and formatting key combinations, a dedicated thread that registers it and receives `WM_HOTKEY` |
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
- The monitor a window is on comes from the rectangle returned by `effective_rect` (`MonitorFromRect`): `GetWindowRect` for normal windows, and `rcNormalPosition` from `GetWindowPlacement` for minimized ones. A minimized window's `GetWindowRect` returns fake coordinates such as (-25600, ...), which can't be used to find its monitor.
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
  - Maximized / minimized window: read `rcNormalPosition` with `GetWindowPlacement` -> map -> `SetWindowPlacement`. Only the "restored position" changes and the show state is kept, so there is no flicker from restoring and re-minimizing.
  - Hidden main windows of minimized windows: the taskbar button of Delphi-style programs is an invisible proxy window (for example PortableApps' `TApplication`), while the real main window (`TfrmMenu`) is another, hidden window in the same process. When moving a minimized window, `hidden_siblings` finds windows of the same process that are hidden, titled, not tool windows and were on the same monitor, and maps and moves them too; otherwise they would reappear at the old position (possibly outside any monitor) the next time they are shown.
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

### Global hotkey (`hotkey.rs`, `app.rs`)

- Default `Ctrl+Shift+M`. Pressing it calls `tray::show_main()`, which shows the main window and brings it to the foreground. It uses Win32 `RegisterHotKey` (with `MOD_NOREPEAT`), so no extra dependency is needed.
- **Dedicated thread**: `RegisterHotKey(NULL, ...)` requires registering and receiving `WM_HOTKEY` on the same thread, and eframe's event loop does not process thread messages. So `hotkey::start` spawns a thread running a `GetMessageW` loop:
  - It first creates the message queue with `PeekMessageW` and then reports its thread ID to the caller (so an early `PostThreadMessageW` can't fail).
  - To change the hotkey, `hotkey::set` sends the new combination and a reply channel into a queue and wakes the thread with `PostThreadMessageW(WM_APP)`; the thread unregisters the old one, registers the new one and synchronously returns whether it worked (failure usually means another program has it).
- **Key format**: `Hotkey { ctrl, alt, shift, vk }`. The text form, such as `Ctrl+Shift+M`, is also the settings file format (`hotkey=Ctrl+Shift+M`); `parse` is case-insensitive. It must include `Ctrl` or `Alt`; the key is limited to `A`-`Z`, `0`-`9` and `F1`-`F12`. `Hotkey::is_common` additionally rejects common shortcuts (only without Shift: `Ctrl+A/C/V/X/Z/Y/S/P/F/O/N/W/T/R`, `Ctrl+0`-`9`, `Ctrl+F4`, `Ctrl+F5` and `Alt+F4`), because a global hotkey takes priority over each program's own shortcuts and would break these system-wide; `parse` rejects them too, so a settings file edited by hand to such a combination falls back to the default. egui keys are turned into virtual-key codes through `Key::name()`. An invalid value in the settings file falls back to the default.
- **Settings panel** (`app.rs`): the keyboard button on the toolbar toggles `hotkey_panel`; the panel is a floating `egui::Area` (`Order::Foreground`, right-aligned just below the toolbar) drawn over the content rather than a `Panel`, so opening it does not squeeze the main window's layout. Its width is capped at 360 and shrinks with a narrow window.
  - Recording: clicking the button sets `capturing = true` and **unregisters the global hotkey first**; otherwise pressing the old combination would be taken by the global hotkey and the window would never see the key. The next `Event::Key` (pressed, not a repeat) is the new combination; `Esc` cancels. Note that egui-winit turns `Ctrl+X/C/V` into `Event::Cut/Copy/Paste` and **does not send `Event::Key` for them**; and when the clipboard has nothing to paste, there is not even a `Paste` event. Relying on egui events alone would miss those combinations, so `poll_capture` also queries the real keys with `hotkey::poll_pressed` (`GetAsyncKeyState`, comparing with the previous state to find the key that was just pressed and reading the current Ctrl / Alt / Shift). A `snapshot` is taken when recording starts so already-held keys are not mistaken for new presses, and the UI repaints every 30 ms while recording so very short presses are not missed. `Esc` still goes through egui events.
  - The error message (`hotkey_msg`) is cleared when recording starts, is cancelled, or the panel is collapsed, so reopening the panel never shows stale red text.
  - Applying: `apply_hotkey` registers first; only on success is it written to the settings, otherwise the old registration is restored and "already used by another program" is shown.
  - Every path out of recording (cancel, collapsing the panel, closing the window to the tray, losing focus) calls `cancel_capture` to restore the registration, so the hotkey is never left unregistered.
- Panel contents: a caption on the first line, then the hotkey button and the "reset to default" icon (`toolbar::icon_reset`, a counter-clockwise circular arrow) on the second, with a fixed row height of 34 so the two are vertically centered. How to use it and the combination rules are in the hotkey button's hover tooltip; the panel only gets an extra red line when something goes wrong.
- Gotcha: at the start of `ui()` a copy `new_settings` is made for the toolbar to change theme and language, and it is compared with `self.settings` and written back at the end. The hotkey is updated by the panel later in the same frame, so `new_settings.hotkey` must be set to the current value before writing back; otherwise the stale copy overwrites the hotkey that was just applied (the UI and the settings file revert to the old value while the new one stays registered).
- A registration failure at startup is reported in the status bar (`Status::HotkeyStartupFailed`) and does not stop the program.

### Single instance (`single_instance.rs`)

- Very early at startup it creates the named mutex `Local\mvApp.SingleInstance`; if it already exists (`ERROR_ALREADY_EXISTS`), another instance is running.
- The first instance also creates an auto-reset named event `Local\mvApp.ShowWindow` and waits on it in a background thread. A second instance just calls `SetEvent` and exits; the first instance then shows its main window.
- The `Local\` prefix limits this to the current login session, so different users can each run one.

### Run at startup (`autostart.rs`)

- It adds a value named `mvApp` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, containing the quoted full path of the current executable. It only writes to the current user's hive and needs no administrator rights.
- If, at startup, it is already enabled but the path differs from the current executable (for example the file was moved), the path is updated.
- The tray menu's check mark reflects the actual registry contents.

### Settings and localization (`settings.rs`, `i18n.rs`)

- The settings file is `%APPDATA%\mvapp\settings.txt`, three `key=value` lines:

  ```
  theme=system|light|dark
  lang=system|zh|en
  hotkey=Ctrl+Shift+M
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
- `hotkey`: formatting and parsing of key combinations (including invalid input and rejection of common shortcuts) and egui key to virtual-key mapping. The actual registration and triggering are not unit-tested and must be verified by hand.
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
