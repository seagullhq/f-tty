# f-tty

Terminale GPU compatto scritto in Rust con [GPUI](https://gpui.rs).

![Screenshot](docs/screenshot.png)

## Caratteristiche

- Tab multiple, ognuna con PTY ed emulatore VTE indipendenti.
- Tab list configurabile su **alto, basso, sinistra o destra**.
- Impostazioni dentro l'area del terminale, senza occupare una tab.
- Selezione del testo col mouse e copia con `Ctrl+Shift+C`.
- Scrollback di 2.000 righe per tab: rotella del mouse e `Shift+PageUp` / `Shift+PageDown`.
- Griglia e PTY ridimensionati in base allo spazio reale della finestra (SIGWINCH).
- Shell rilevate automaticamente (`/etc/shells` su Unix, `COMSPEC`/PATH su Windows).
- Barra del titolo custom: controlli tondi su Linux, caption a piena altezza su Windows, traffic lights native su macOS.
- Tema scuro flat, angoli arrotondati, UI in italiano.

## Requisiti

- Rust stable recente (edition 2024).
- **Linux**: driver Vulkan e librerie di sviluppo per fontconfig, freetype, xkbcommon e Wayland/X11.
  - Debian/Ubuntu: `sudo apt install gcc g++ clang cmake pkg-config libfontconfig-dev libfreetype-dev libwayland-dev wayland-protocols libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libssl-dev libvulkan-dev libvulkan1`
  - Fedora: `sudo dnf install gcc-c++ clang cmake pkg-config fontconfig-devel freetype-devel wayland-devel libxkbcommon-devel libxkbcommon-x11-devel libX11-devel libxcb-devel vulkan-devel`
- **Windows**: toolchain Rust MSVC con Visual Studio Build Tools (oppure toolchain GNU con MinGW); serve un compilatore C per le dipendenze.
- **macOS**: Xcode e Command Line Tools.

## Build

```sh
cargo build --release
./target/release/ftty
```

## Scorciatoie

| Azione | Tasto |
| --- | --- |
| Nuova tab | `Ctrl+Shift+T` |
| Chiudi tab | `Ctrl+Shift+W` |
| Tab successiva / precedente | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Copia selezione | `Ctrl+Shift+C` |
| Scorri lo storico | rotella, `Shift+PageUp` / `Shift+PageDown` |
| Torna al presente | digita un tasto o scorri fino in fondo |
| Apri/chiudi impostazioni | pulsante ingranaggio |

## Stato

Testato su Linux (KDE Wayland). Il codice per Windows è stato verificato con
`cargo check --target x86_64-pc-windows-gnu`, ma l'app non è ancora stata
eseguita su hardware Windows; macOS non è stato verificato.

## Licenza

Le icone in `assets/icons/` sono Heroicons, licenza MIT (vedi
`assets/icons/LICENSE`). La licenza del progetto non è ancora stata scelta.
