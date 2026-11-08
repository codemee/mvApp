**English** | [繁體中文](README.md)

# mvApp

A Windows-only tool (Rust + egui) for moving application windows between monitors.

It shows one pane per active monitor, lists the GUI windows on that monitor as application icons, and lets you drag an icon onto another monitor's pane to move the window there.

## Features
- One pane per monitor (in physical left-to-right order), with a monitor icon before the title, showing resolution and the primary monitor.
- When the window is too narrow, the panes automatically stack vertically.
- Lists visible top-level windows (excludes tool windows, cloaked windows, the desktop and mvApp itself).
- Drag-and-drop move: keeps relative position and size; maximized windows are re-maximized after moving; minimized windows stay minimized.
- Click an icon to bring that window to the foreground.
- Auto refresh; monitor hot-plug is supported.
- Lives in the system tray: it starts in the tray only; double-click the tray icon to open the main window; the close button sends it back to the tray. The tray's right-click menu has "Run at startup" (a checkable toggle that writes to the current user's `HKCU\...\Run`, no administrator rights needed) and "Exit" (the only way to really quit).
- Single instance: if mvApp is already running, launching it again just brings the existing main window forward and the new process exits (per Windows login session).
- The toolbar has Refresh on the left and two flat three-state buttons on the right; each click moves to the next state: theme (follow system [half-filled circle] -> light [sun] -> dark [moon]) and language (follow system [globe] -> Traditional Chinese [中] -> English [En]). Settings are saved to `%APPDATA%\mvapp\settings.txt`.
- A lightweight Windows 11 Fluent-style look: neutral background, thin-bordered groups (title set into the border), a white list area inside, and gray thin-line icons. The light theme takes its accent colors from the Gundam RX-78-2 (blue, red, yellow) and the dark theme from Neon Genesis Evangelion Unit-01 (dark purple, neon green, orange); they are used for the monitor icons and the drop highlight.

## Download and use
1. Download `mvapp.exe` from the [Releases](../../releases) page (a single file, no installer, Windows x64).
2. Double-click to run. It starts in the system tray only; double-click the tray icon to open the main window.
3. To start it with Windows: right-click the tray icon and check "Run at startup".

> The executable is not code-signed, so Windows SmartScreen may show a warning the first time; choose "More info" -> "Run anyway".

## Build and run
```powershell
cargo run --release
cargo test
```

## Limitations
- Windows running as administrator can only be moved if mvapp is also started as administrator (UIPI).
- Switching windows across multiple virtual desktops is not supported.
