**English** | [繁體中文](README.md)

# mvApp

A small Windows tool for moving a window from one monitor to another by dragging.

Open the main window and you get one pane per monitor, listing the windows currently on that monitor as application icons. Drag an icon onto another monitor's pane and release it, and the window moves there.

## Download and use
1. Download `mvapp.exe` from the [Releases](../../releases) page (a single file, no installer, Windows x64).
2. Double-click to run. It starts in the system tray only (the bottom-right of the taskbar); **double-click the tray icon** to open the main window.
3. To start it with Windows: right-click the tray icon and check "Run at startup".

> The executable is not code-signed, so Windows SmartScreen may show a warning the first time; choose "More info" -> "Run anyway".

## How to use it
- **Move a window**: drag its icon onto another monitor's pane and release. The window keeps its relative position and size; a maximized window stays maximized after moving, and a minimized one stays minimized.
- **Switch to a window**: click its icon.
- **Toolbar**:
  - On the left is "Refresh".
  - On the right are two buttons. **Each click moves to the next state**, and hovering shows the current state:
    - Theme: follow system (half-filled circle) -> light (sun) -> dark (moon).
    - Language: follow system (globe) -> Traditional Chinese (中) -> English (En).
- **System tray**: closing the window does not quit the program; it just goes back to the tray. To really quit, right-click the tray icon and choose "Exit".

## Highlights
- One pane per monitor, in physical left-to-right order, showing each monitor's resolution and which one is primary.
- When the window is too narrow, the panes automatically stack vertically.
- The window list updates automatically, including when you plug in or unplug a monitor.
- Single instance: launching it again while it is running just brings the existing main window forward.
- The light theme takes its colors from the Gundam RX-78-2, and the dark theme from Neon Genesis Evangelion Unit-01.

## FAQ
**I don't see a window after starting it.**
That is normal; it starts in the system tray only. Double-click the tray icon to open it. If you don't see the icon, click the "Show hidden icons" arrow in the tray.

**Some windows can't be moved.**
A program running as administrator can only be moved if mvApp is also running as administrator.

**How do I reset my settings?**
Your theme and language are remembered automatically. Quit the program and delete `%APPDATA%\mvapp\settings.txt` to restore the defaults.

## Limitations
- Windows only (developed and tested on Windows 11).
- Moving windows between multiple virtual desktops is not supported.

## License
This project is licensed under the [GNU General Public License, version 3 or later (GPL-3.0-or-later)](LICENSE). You may use, study, modify and redistribute it under that license; when you distribute it (including modified versions), you must make the corresponding source code available under the GPL as well. See [LICENSE](LICENSE) for the full text.

## For developers
See the [Development and technical notes](docs/DEVELOPMENT.en.md) for build instructions, project structure and technical details.
