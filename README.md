# ViKey — Standalone Rust Vietnamese Input Method for Fedora & Niri

ViKey is a lightweight, standalone Vietnamese Input Method Editor (IME) written in pure Rust for **Fedora Linux** and the **Niri** scrolling tiling Wayland compositor.

Unlike traditional Linux IMEs, ViKey speaks `zwp_input_method_v2` and `zwp_virtual_keyboard_v1` directly to Niri without requiring IBus or Fcitx5, and connects to `$NIRI_SOCKET` for intelligent per-window language memory and per-application commit strategies.

## Features

- **Standalone Wayland Daemon (`vikey-daemon`)**: Uses `zwp_input_method_v2` + `zwp_virtual_keyboard_v1` directly with Niri.
- **Dual Commit Engine (Per-App Configurable)**:
  - `virtual_backspace`: Windows UniKey / `ibus-bamboo` style immediate character commit with minimal-diff synthetic backspace replacement (no pre-edit underline; works cleanly in browser URL bars, Electron apps, VS Code, Discord).
  - `surrounding_text`: Immediate character commit using `zwp_input_method_v2::delete_surrounding_text`.
  - `preedit`: Standard Wayland underlined composition buffer.
- **Complete Linguistic Engine (`vikey-core`)**:
  - **Telex** & **VNI** input methods with free-order tone/diacritic placement and double-tap escape undo (`aa` -> `a`, `dd` -> `d`, `ww` -> `w`, `ss` -> `s`).
  - **Modern** (`hoà`, `hoè`, `thuý`) and **Traditional** (`hòa`, `hòe`, `thúy`) tone placement.
  - **Vietnamese Phonotactic Spellchecker & English Auto-Restore**: Automatically preserves English words (`window`, `wayland`, `restore`, `fedora`, `firefox`, `rust`, `support`, `text`, etc.) without switching modes.
- **Native Niri IPC Integration (`$NIRI_SOCKET`)**:
  - Remembers `VN` / `EN` state per Niri window.
  - Automatically switches to `EN` in terminal emulators (`foot`, `Alacritty`, `kitty`, `wezterm`, `ghostty`, `Ptyxis`) and Steam games (`steam_app_*`).
- **System Tray, Pure Rust GUI & CLI**:
  - Built-in DBus `StatusNotifierItem` tray badge (`VN` / `EN`) for Waybar / Cosmic Panel.
  - `vikey-gui`: Pure Rust `egui`/`eframe` settings panel with a live Niri running-app rule picker and test sandbox.
  - `vikey-ctl`: Fast CLI for Niri keybinds (`vikey-ctl toggle`) and Waybar custom module streaming (`vikey-ctl status --watch --json`).

## Installation on Fedora Linux

```bash
cargo install --path crates/vikey-daemon
cargo install --path crates/vikey-cli
cargo install --path crates/vikey-gui
```

## Niri Configuration (`~/.config/niri/config.kdl`)

See [`packaging/niri-snippet.kdl`](packaging/niri-snippet.kdl).

> **Note for Fedora Wayland:** Set `GTK_IM_MODULE=wayland` and `QT_IM_MODULE=wayland` (and stop `fcitx5`/`ibus-daemon`) so GTK and Qt applications use the native Wayland `zwp_text_input_v3` protocol directly with Niri. For Flatpak apps (e.g. Zen Browser), also run:
>
> ```bash
> flatpak override --user --env=GTK_IM_MODULE=wayland --env=QT_IM_MODULE=wayland
> ```

## License

Released under the [MIT License](LICENSE).
