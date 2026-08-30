%global debug_package %{nil}

Name:           anyflow
Version:        0.1.0
Release:        1%{?dist}
Summary:        Local-first device continuity between Android and Fedora

License:        Apache-2.0
URL:            https://github.com/yurisismotto/anyflow
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.82
BuildRequires:  cargo
BuildRequires:  gcc
# No protobuf-compiler: the build uses protox, a pure-Rust protobuf compiler.
# See docs/adr/ADR-0004-protocol-buffers.md.

# UPower is a weak dependency: without it the daemon simply does not report
# this machine's own battery, which is the normal case on a desktop tower.
Recommends:     upower

%description
AnyFlow connects an Android phone to a Fedora workstation over the
local network. It is local-first: there is no cloud service, no account and
no telemetry. Devices authenticate each other with pinned public keys over
TLS 1.3 after an explicit, human-confirmed pairing.

This package provides the user-session daemon and the anyflow command-line
tool. The daemon runs unprivileged under systemd --user and never requires
root.

%prep
%autosetup

%build
cd desktop
cargo build --release --locked

%install
install -Dpm0755 desktop/target/release/anyflowd %{buildroot}%{_bindir}/anyflowd
install -Dpm0755 desktop/target/release/anyflow  %{buildroot}%{_bindir}/anyflow
install -Dpm0644 packaging/fedora/anyflowd.service \
    %{buildroot}%{_userunitdir}/anyflowd.service

%check
cd desktop
cargo test --release --locked

%files
%license LICENSE
%doc README.md docs/
%{_bindir}/anyflowd
%{_bindir}/anyflow
%{_userunitdir}/anyflowd.service

%post
cat <<'EOF'
AnyFlow installed. To start it for your user:

    systemctl --user enable --now anyflowd.service
    anyflow status

Then pair your phone:

    anyflow pair

The daemon runs as your user and never needs root.
EOF

%changelog
* Sat Aug 29 2026 Yuri Converso Sismotto <yuri.sismotto@gmail.com> - 0.1.0-1
- Initial package: protocol foundation, pairing, authenticated transport,
  battery.v1
