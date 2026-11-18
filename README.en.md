**English** | [繁體中文](README.md)

# mvApp

A small Windows tool for moving a window from one monitor to another by dragging.

Open the main window and you get one pane per monitor, listing the windows currently on that monitor as application icons. Drag an icon onto another monitor's pane and release it, and the window moves there.

## Download and use
1. Download `mvapp.exe` from the [Releases](../../releases) page (a single file, no installer, Windows x64).
2. Double-click to run. It starts in the system tray only (the bottom-right of the taskbar); **double-click the tray icon** to open the main window.
3. From then on, press **Ctrl+Shift+M** anywhere (customizable, see below) to bring up the main window.
4. To start it with Windows: right-click the tray icon and check "Run at startup".

> The executable is not code-signed, so Windows SmartScreen may show a warning the first time; choose "More info" -> "Run anyway".

## How to use it
- **Move a window**: drag its icon onto another monitor's pane and release. The window keeps its relative position and size; a maximized window stays maximized after moving, and a minimized one stays minimized.
- **Switch to a window**: click its icon.
- **Toolbar**:
  - On the left is "Refresh".
  - On the right, in order, are "Hotkey settings" (the keyboard icon), language and theme:
    - Hotkey settings: click to expand or collapse the settings panel (see below).
    - Language: follow system (globe) -> Traditional Chinese (中) -> English (En).
    - Theme: follow system (half-filled circle) -> light (sun) -> dark (moon).
    - **Each click on language or theme moves to the next state**, and hovering shows the current state.
- **Hotkey**: click the keyboard button on the toolbar to open a floating settings panel below it (it sits on top of the main window and does not squeeze the layout); click the button again to close it.
  1. Click the button showing the current combination (default `Ctrl+Shift+M`); it changes to "Press the new hotkey…".
  2. Press the combination you want and it is applied; press `Esc` to cancel.
  3. To go back to the default, click the circular-arrow button next to it (hover it for a description).
  - Rules (also shown when you hover the hotkey button): it must include `Ctrl` or `Alt`, and the key can be `A`-`Z`, `0`-`9` or `F1`-`F12`.
  - So that features in other programs keep working, **common shortcuts are not allowed**: plain `Ctrl+A` / `C` / `V` / `X` / `Z` / `Y` / `S` / `P` / `F` / `O` / `N` / `W` / `T` / `R`, `Ctrl+0`-`9`, `Ctrl+F4`, `Ctrl+F5` and `Alt+F4`. Adding `Shift` or another modifier (for example `Ctrl+Shift+C`) makes it allowed.
  - If another program already uses the combination, you are asked to pick another and the previous hotkey stays in effect.
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

**Nothing happens when I press the hotkey.**
Most likely another program already uses that combination. If this happens at startup, the status bar says so; click the keyboard button on the toolbar and pick another one.

**Some windows can't be moved.**
A program running as administrator can only be moved if mvApp is also running as administrator.

**How do I reset my settings?**
Your theme, language and hotkey are remembered automatically. Quit the program and delete `%APPDATA%\mvapp\settings.txt` to restore the defaults.

## Limitations
- Windows only (developed and tested on Windows 11).
- Moving windows between multiple virtual desktops is not supported.

## License
This project is licensed under the [GNU General Public License, version 3 or later (GPL-3.0-or-later)](LICENSE). You may use, study, modify and redistribute it under that license; when you distribute it (including modified versions), you must make the corresponding source code available under the GPL as well. See [LICENSE](LICENSE) for the full text.

## For developers
See the [Development and technical notes](docs/DEVELOPMENT.en.md) for build instructions, project structure and technical details.
