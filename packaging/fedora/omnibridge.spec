# No -debuginfo / -debugsource subpackage for v1. Recorded as a deliberate
# choice, not an oversight:
# docs/audits/packaging/PACKAGING-V1-READINESS-AUDIT.md §19 Q2.
%global debug_package %{nil}

Name:           omnibridge
# AUTHORITATIVE SOURCE: desktop/Cargo.toml [workspace.package] version.
#
# rpm cannot read a TOML file at spec-parse time, and an SRPM does not carry
# Cargo.toml at the point Version: is needed, so this is a literal copy. It is
# not an unguarded one: packaging/release/make-source-bundle.sh refuses to
# build a bundle when the two disagree, and packaging/tests/packaging-checks.sh
# asserts it without building anything. Audit §13.1.
Version:        0.1.0
Release:        2%{?dist}
Summary:        Local-first device continuity between Android and Fedora

License:        Apache-2.0
URL:            https://github.com/yurisismotto/omnibridge

# Both tarballs come from packaging/release/make-source-bundle.sh. Source1 is
# every crate in desktop/Cargo.lock, vendored, because mock and koji build
# with networking switched off and `cargo build --locked` still downloads.
# That was defect B3 (audit §5.1) and it made this package unbuildable.
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

# The Rust floor is measured from the committed lockfile, not chosen: `time`
# 0.3.55 demands 1.88 and cargo's resolver refuses before compiling a line.
# desktop/Cargo.toml carries the full derivation. This line used to say 1.82,
# which the workspace's own comment records as having been false (audit P1).
BuildRequires:  rust >= 1.88
BuildRequires:  cargo

# --- native toolchain ------------------------------------------------------
BuildRequires:  gcc

# --- GUI build dependencies (audit B1) -------------------------------------
#
# %%build compiles omnibridge-gui, and it used to do so with none of this
# declared, so the package could not build at all. Each line below is here
# because something in the build asks for it by name:
#
#   pkgconf-pkg-config  gtk4-sys, libadwaita-sys and the six other -sys crates
#                       resolve their C libraries through `system-deps`, which
#                       shells out to pkg-config. gtk4-devel also Requires
#                       /usr/bin/pkg-config directly.
#   gtk4-devel          pkg-config module `gtk4` >= 4.12 (the v4_12 feature in
#                       desktop/gui/Cargo.toml gates CssProvider::load_from_string).
#                       Pulls pango-devel, graphene-devel and gdk-pixbuf2-devel,
#                       which gdk4-sys, gsk4-sys, pango-sys, graphene-sys and
#                       gdk-pixbuf-sys need, so those are not repeated here.
#   libadwaita-devel    pkg-config module `libadwaita-1` >= 1.5 (adw::Dialog and
#                       adw::AlertDialog do not exist at 1.4).
#   glib2-devel         two separate reasons: the `glib-2.0`, `gio-2.0` and
#                       `gobject-2.0` pkg-config modules, and the
#                       glib-compile-resources binary that desktop/gui/build.rs
#                       runs through glib-build-tools to compile the gresource
#                       bundle into the binary.
#
# Nothing here was added speculatively. The set was derived from the
# [package.metadata.system-deps] tables of the locked -sys crates, mapped to
# owning packages with rpm -qf, and then proved minimal by building in a
# container that had only these installed.
BuildRequires:  pkgconf-pkg-config
BuildRequires:  gtk4-devel >= 4.12
BuildRequires:  libadwaita-devel >= 1.5
BuildRequires:  glib2-devel

# --- systemd macros (audit B2) ---------------------------------------------
#
# %%{_userunitdir} comes from this package and from nowhere else. Without it
# the macro does not expand, %%install creates a directory whose name is the
# literal string '%%{_userunitdir}', and %%files fails on the path that is
# missing. Declaring it is the fix; hardcoding /usr/lib/systemd/user would
# bypass the symptom and lose the distro-correct path.
BuildRequires:  systemd-rpm-macros

# --- %%check dependencies ---------------------------------------------------
#
# platform-linux/tests/{tray_dbus,tray_gnome}.rs do not mock the bus. Each one
# raises its own private dbus-daemon with no service directories, publishes a
# real StatusNotifierItem on it and drives a fake watcher with KDE's own
# signatures — which is the only way those tests can say anything true about
# what Plasma and GNOME will do. The fixture shells out to the binary by name
# and says so itself: `.expect("dbus-daemon should be installed")`,
# platform-linux/tests/common/mod.rs:68.
#
# Nothing else in the suite needs a binary the buildroot lacks: the full set
# it shells out to is /bin/sh, id, loginctl and dbus-daemon, and the first
# three are already there. The tests that want a *session* bus, a display, a
# real UPower or a real clipboard are #[ignore]d and stay skipped here.
BuildRequires:  dbus-daemon

# No protobuf-compiler: the build uses protox, a pure-Rust protobuf compiler.
# See docs/adr/ADR-0004-protocol-buffers.md.

# UPower is a weak dependency: without it the daemon simply does not report
# this machine's own battery, which is the normal case on a desktop tower.
Recommends:     upower

%description
OmniBridge connects an Android phone to a Fedora workstation over the
local network. It is local-first: there is no cloud service, no account and
no telemetry. Devices authenticate each other with pinned public keys over
TLS 1.3 after an explicit, human-confirmed pairing.

This package provides the user-session daemon, the omnibridge command-line
tool and the desktop application. The daemon runs unprivileged under
systemd --user and never requires root.

%prep
%autosetup -n %{name}-%{version}

# Unpack the vendored crates next to the workspace and point cargo at them.
# From here on the build resolves every dependency from this tree, which is
# what makes it survive a network-isolated buildroot.
tar -xf %{SOURCE1} -C desktop
install -Dpm0644 packaging/fedora/cargo-vendor-config.toml desktop/.cargo/config.toml

%build
# CARGO_HOME is redirected into the build tree so no ~/.cargo/config.toml
# belonging to whoever runs rpmbuild can quietly reintroduce a registry. With
# that plus --offline, a network fetch is not merely unnecessary here, it is
# impossible — which is the property B3 was missing.
export CARGO_HOME=%{_builddir}/%{name}-cargo-home

cd desktop
# Explicit targets rather than --workspace: the package ships exactly these
# three binaries, and naming them is what keeps the build from drifting into
# compiling members the package does not install. They already pull in every
# workspace library between them, so nothing is lost.
cargo build --release --locked --offline \
    -p omnibridge-daemon \
    -p omnibridge-cli \
    -p omnibridge-gui

%install
install -Dpm0755 desktop/target/release/omnibridged   %{buildroot}%{_bindir}/omnibridged
install -Dpm0755 desktop/target/release/omnibridge    %{buildroot}%{_bindir}/omnibridge
# The GUI was built by %%build and then discarded before this sprint (audit
# P7). Installing the binary is the floor; the .desktop entry, the hicolor
# icon, the D-Bus activation file and the omnibridge-gui subpackage split are
# Phase 3 and are not done here.
install -Dpm0755 desktop/target/release/omnibridge-gui %{buildroot}%{_bindir}/omnibridge-gui
install -Dpm0644 packaging/fedora/omnibridged.service \
    %{buildroot}%{_userunitdir}/omnibridged.service

%check
export CARGO_HOME=%{_builddir}/%{name}-cargo-home
cd desktop
cargo test --release --locked --offline

%files
%license LICENSE
%doc README.md docs/
%{_bindir}/omnibridged
%{_bindir}/omnibridge
%{_bindir}/omnibridge-gui
%{_userunitdir}/omnibridged.service

%post
cat <<'EOF'
OmniBridge installed. To start it for your user:

    systemctl --user enable --now omnibridged.service
    omnibridge status

Then pair your phone:

    omnibridge pair

The daemon runs as your user and never needs root.
EOF

%changelog
* Tue Sep 22 2026 Yuri Converso Sismotto <yuri.sismotto@gmail.com> - 0.1.0-2
- Make the package buildable. 0.1.0-1 never produced an artifact: it compiled
  omnibridge-gui with no GTK or libadwaita build dependency declared (B1),
  used %%{_userunitdir} without BuildRequires: systemd-rpm-macros (B2), and
  ran cargo against crates.io inside a network-isolated buildroot (B3).
- Build offline from a vendored source bundle; install the GUI binary rather
  than compiling and discarding it.
* Sat Aug 29 2026 Yuri Converso Sismotto <yuri.sismotto@gmail.com> - 0.1.0-1
- Initial package: protocol foundation, pairing, authenticated transport,
  battery.v1
