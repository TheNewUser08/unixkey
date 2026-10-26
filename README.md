# UniXKey — Standalone Rust Vietnamese Input Method for Fedora & Niri

<p align="center">
  <a href="https://ko-fi.com/dang809156"><img src="https://img.shields.io/badge/Ko--fi-Support%20Author-F16061?style=for-the-badge&logo=ko-fi&logoColor=white" alt="Support on Ko-fi"></a>
  <a href="#-chuyển-khoản-qua-vietqr-bidv"><img src="https://img.shields.io/badge/VietQR-Ủng%20hộ%20BIDV-006B68?style=for-the-badge" alt="VietQR BIDV"></a>
  <img src="https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge" alt="MIT License">
  <img src="https://img.shields.io/badge/Fedora-Wayland%20%2F%20Niri-51A2DA?style=for-the-badge&logo=fedora&logoColor=white" alt="Fedora Niri">
</p>

UniXKey is a lightweight, standalone Vietnamese Input Method Editor (IME) written in pure Rust for **Fedora Linux** and the **Niri** scrolling tiling Wayland compositor.

Unlike traditional Linux IMEs, UniXKey speaks `zwp_input_method_v2` and `zwp_virtual_keyboard_v1` directly to Niri without requiring IBus or Fcitx5, and connects to `$NIRI_SOCKET` for intelligent per-window language memory and per-application commit strategies.

## Features

- **Standalone Wayland Daemon (`unixkey-daemon`)**: Uses `zwp_input_method_v2` + `zwp_virtual_keyboard_v1` directly with Niri.
- **Dual Commit Engine (Per-App Configurable)**:
  - **`direct` (Direct Commit)**: Commits characters directly without underline, replacing text atomically via `delete_surrounding_text` (virtual-keyboard Backspace fallback for apps without surrounding-text support). Enables live IDE code recommendations (VS Code, Zed) and real-time browser search suggestions as you type.
  - **`preedit` (Pre-edit Buffer)**: Standard Wayland underlined composition buffer (`zwp_input_method_v2::set_preedit_string`) with full protocol compliance.
- **Complete Linguistic Engine (`unixkey-core`)**:
  - **Telex** & **VNI** input methods with free-order tone/diacritic placement and double-tap escape undo (`aa` -> `a`, `dd` -> `d`, `ww` -> `w`, `ss` -> `s`).
  - **Modern** (`hoà`, `hoè`, `thuý`) and **Traditional** (`hòa`, `hòe`, `thúy`) tone placement.
  - **Vietnamese Phonotactic Spellchecker & English Auto-Restore**: Automatically preserves English words (`window`, `wayland`, `restore`, `fedora`, `firefox`, `rust`, `support`, `text`, etc.) without switching modes.
- **Native Niri IPC Integration (`$NIRI_SOCKET`)**:
  - Remembers `VN` / `EN` state per Niri window.
  - Automatically switches to `EN` in terminal emulators (`foot`, `Alacritty`, `kitty`, `wezterm`, `ghostty`, `Ptyxis`) and Steam games (`steam_app_*`).
- **System Tray, Pure Rust GUI & CLI**:
  - Built-in DBus `StatusNotifierItem` tray badge (`VN` / `EN`) for Waybar / Cosmic Panel.
  - `unixkey-gui`: Pure Rust `egui`/`eframe` settings panel with a live Niri running-app rule picker and test sandbox.
  - `unixkey-ctl` (or `unixkey`): Fast CLI for Niri keybinds (`unixkey-ctl toggle`) and Waybar custom module streaming (`unixkey-ctl status --watch --json`).

## Installation on Fedora Linux

### Option 1: Via Fedora COPR (Recommended)

```bash
sudo dnf copr enable thenewuser08/unixkey
sudo dnf install unixkey
```

### Option 2: Build from Source (Cargo)

```bash
cargo install --path crates/unixkey-daemon
cargo install --path crates/unixkey-cli
cargo install --path crates/unixkey-gui
```

## Niri Configuration (`~/.config/niri/config.kdl`)

See [`packaging/niri-snippet.kdl`](packaging/niri-snippet.kdl).

> **Note for Fedora Wayland:** Set `GTK_IM_MODULE=wayland` and `QT_IM_MODULE=wayland` (and stop `fcitx5`/`ibus-daemon`) so GTK and Qt applications use the native Wayland `zwp_text_input_v3` protocol directly with Niri. For Flatpak apps (e.g. Zen Browser), also run:
>
> ```bash
> flatpak override --user --env=GTK_IM_MODULE=wayland --env=QT_IM_MODULE=wayland
> ```

## Support / Ủng hộ tác giả ☕

Nếu bạn thấy UniXKey hữu ích cho trải nghiệm gõ tiếng Việt trên Linux và Niri, bạn có thể ủng hộ dự án qua:

### 🇻🇳 Chuyển khoản qua VietQR (BIDV)

<div align="center">
  <img src="assets/vietqr.jpg" width="220" alt="VietQR BIDV 8850896383" />
  <br/>
  <b>Ngân hàng TMCP Đầu tư và Phát triển Việt Nam (BIDV)</b><br/>
  Số tài khoản: <code>8850896383</code><br/>
  Chủ tài khoản: <b>NGUYEN HAI DANG</b>
</div>

<br/>

### 🌍 International Supporters (Ko-fi)

<p align="center">
  <a href="https://ko-fi.com/dang809156" target="_blank">
    <img src="https://storage.ko-fi.com/cdn/kofi3.png?v=3" height="40" style="border:0px;height:40px;" alt="Support on Ko-fi" />
  </a>
  <br/>
  <a href="https://ko-fi.com/dang809156">https://ko-fi.com/dang809156</a>
</p>

## License

Released under the [MIT License](LICENSE).
