# f-tty

A compact GPU terminal written in Rust with [GPUI](https://gpui.rs).

![Screenshot](docs/screenshot.png)

## Features

- Multiple tabs, each with its own PTY and VTE emulator.
- Tab list position: **top, bottom, left or right**.
- Settings inside the terminal area, without taking up a tab.
- Mouse text selection and copy with `Ctrl+Shift+C`.
- 2,000-line scrollback per tab: mouse wheel and `Shift+PageUp` / `Shift+PageDown`.
- Grid and PTY resized to the actual window space (SIGWINCH).
- Automatic shell detection (`/etc/shells` on Unix, `COMSPEC`/PATH on Windows).
- Custom title bar: round controls on Linux, full-height captions on Windows, native traffic lights on macOS.
- Flat dark theme, rounded corners, English UI.

## Requirements

- Recent stable Rust (edition 2024).
- **Linux**: Vulkan driver and development libraries for fontconfig, freetype, xkbcommon and Wayland/X11.
  - Debian/Ubuntu: `sudo apt install gcc g++ clang cmake pkg-config libfontconfig-dev libfreetype-dev libwayland-dev wayland-protocols libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libssl-dev libvulkan-dev libvulkan1`
  - Fedora: `sudo dnf install gcc-c++ clang cmake pkg-config fontconfig-devel freetype-devel wayland-devel libxkbcommon-devel libxkbcommon-x11-devel libX11-devel libxcb-devel vulkan-devel`
- **Windows**: Rust MSVC toolchain with Visual Studio Build Tools (or GNU toolchain with MinGW); a C compiler is required for dependencies.
- **macOS**: Xcode and Command Line Tools.

## Build

```sh
cargo build --release
./target/release/ftty
```

## Shortcuts

| Action | Key |
| --- | --- |
| New tab | `Ctrl+Shift+T` |
| Close tab | `Ctrl+Shift+W` |
| Next / previous tab | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Copy selection | `Ctrl+Shift+C` |
| Scroll history | mouse wheel, `Shift+PageUp` / `Shift+PageDown` |
| Back to the present | type a key or scroll to the bottom |
| Open/close settings | gear button |

## Status

Tested on Linux (KDE Wayland). The Windows code has been checked with
`cargo check --target x86_64-pc-windows-gnu`, but the app has not been run on
Windows hardware yet; macOS has not been verified.

## License

The icons in `assets/icons/` are Heroicons, MIT licensed (see
`assets/icons/LICENSE`). The project license has not been chosen yet.
