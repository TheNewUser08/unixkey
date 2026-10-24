Name:           unixkey
Version:        0.1.0
Release:        1%{?dist}
Summary:        Standalone Rust Vietnamese Input Method for Wayland & Niri

License:        MIT
URL:            https://github.com/TheNewUser08/unixkey
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig(xkbcommon)
BuildRequires:  pkgconfig(wayland-client)
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils

Requires:       xkbcomp

%description
UniXKey is a lightweight, standalone Vietnamese Input Method Editor (IME)
written in pure Rust for Fedora Linux and the Niri Wayland scrolling compositor.
It speaks zwp_input_method_v2 and zwp_virtual_keyboard_v1 directly to Niri
without requiring IBus or Fcitx5.

%prep
%autosetup -n %{name}-%{version}

%build
cargo build --release

%install
# Binaries
install -D -p -m 0755 target/release/unixkey-daemon %{buildroot}%{_bindir}/unixkey-daemon
install -D -p -m 0755 target/release/unixkey-ctl %{buildroot}%{_bindir}/unixkey-ctl
install -D -p -m 0755 target/release/unixkey %{buildroot}%{_bindir}/unixkey
install -D -p -m 0755 target/release/unixkey-gui %{buildroot}%{_bindir}/unixkey-gui

# Backwards compatibility symlinks
ln -sf unixkey-daemon %{buildroot}%{_bindir}/vikey-daemon
ln -sf unixkey-ctl %{buildroot}%{_bindir}/vikey-ctl
ln -sf unixkey-gui %{buildroot}%{_bindir}/vikey-gui

# Desktop launcher
install -D -p -m 0644 packaging/unixkey.desktop %{buildroot}%{_datadir}/applications/unixkey.desktop

# Icons
install -D -p -m 0644 assets/unixkey.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/unixkey.svg
install -D -p -m 0644 assets/unixkey.png %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/unixkey.png

# Systemd user service
install -D -p -m 0644 packaging/unixkey.service %{buildroot}%{_userunitdir}/unixkey-daemon.service

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/unixkey.desktop

%post
%systemd_user_post unixkey-daemon.service

%preun
%systemd_user_preun unixkey-daemon.service

%files
%license LICENSE
%doc README.md
%{_bindir}/unixkey-daemon
%{_bindir}/unixkey-ctl
%{_bindir}/unixkey
%{_bindir}/unixkey-gui
%{_bindir}/vikey-daemon
%{_bindir}/vikey-ctl
%{_bindir}/vikey-gui
%{_datadir}/applications/unixkey.desktop
%{_datadir}/icons/hicolor/scalable/apps/unixkey.svg
%{_datadir}/icons/hicolor/256x256/apps/unixkey.png
%{_userunitdir}/unixkey-daemon.service

%changelog
* Sat Oct 24 2026 Nguyễn Hải Đăng <thenewuser08> - 0.1.0-1
- Initial release of UniXKey for Fedora & Niri
