#!/usr/bin/env bash
#
# Installs OmniBridge's desktop entry, application icon and D-Bus activation
# entry into an XDG data directory.
#
# ---------------------------------------------------------------------------
# Why this exists
# ---------------------------------------------------------------------------
#
# On a Wayland session the shell — not the application — decides what icon a
# window gets, and it has exactly one way to work it out:
#
#     xdg_toplevel.set_app_id("io.github.yurisismotto.omnibridge")
#         -> the .desktop file with that id, from XDG_DATA_DIRS
#             -> its Icon= name
#                 -> that name in the *shell's* icon theme
#
# Every link in that chain is outside this process. An icon compiled into the
# OmniBridge binary is private to OmniBridge, and `gtk_window_set_default_icon_name`
# has no transport at all under xdg-shell: Mutter advertises no
# `xdg_toplevel_icon_manager_v1`, and there is no Wayland equivalent of X11's
# `_NET_WM_ICON`. So on Wayland the window falls back to a generic glyph until
# these two files exist somewhere the session looks.
#
# This is therefore not a convenience script. It is the whole mechanism, and
# packaging needs the same files in the same places under a different prefix —
# which is why the prefix is an argument and the two identity files are
# installed verbatim rather than rewritten. Identity cannot drift between a
# development run and a package if neither one edits the file.
#
# The third file, the D-Bus activation entry, is the one exception and is a
# template for a reason the file's own header explains: a service file's
# `Exec` must be an absolute path, so exactly one line has to be derived from
# the prefix. Nothing about OmniBridge's *identity* is derived — the bus name in
# it is the same literal as everywhere else.
#
# ---------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------
#
#   ./install-desktop-metadata.sh                     # into ~/.local
#   ./install-desktop-metadata.sh --link-binary PATH  # ...and put PATH on PATH
#   ./install-desktop-metadata.sh --uninstall
#
#   # what packaging does, writing into a buildroot and never into a live /usr:
#   ./install-desktop-metadata.sh --prefix /usr --destdir "$RPM_BUILD_ROOT"
#
# It needs no root, touches nothing outside the prefix it is given, and is not
# run by the application: OmniBridge never installs anything at startup.

set -euo pipefail

# Must match APP_ID in ../src/lib.rs, the Wayland app_id, the D-Bus name, the
# desktop file's basename, its Icon= key, the D-Bus service file's basename
# and its Name= key, and the tray item's IconName. One string, and tests in
# both `omnibridge-gui` and `omnibridge-linux` assert every copy of it agrees.
APP_ID="io.github.yurisismotto.omnibridge"

here() { cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd; }
TOOLS="$(here)"
DESKTOP_SRC="$TOOLS/../data/$APP_ID.desktop"
# The D-Bus activation entry. A template, because `Exec` in a service file has
# to be an absolute path and the only honest source of one is the prefix this
# script was given. See the file's own header for why `DBusActivatable=true`
# is not enough on its own.
DBUS_SRC="$TOOLS/../data/$APP_ID.service.in"
# The canonical mark, from the one place the brand documentation points at.
# build.rs derives the compiled-in copy from this same file.
ICON_SRC="$TOOLS/../../../docs/design/assets/omnibridge-app-icon.svg"

prefix="${HOME}/.local"
destdir=""
uninstall=0
link_binary=""

die() { printf 'install-desktop-metadata: %s\n' "$1" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix)  [ $# -ge 2 ] || die "--prefix needs a directory"; prefix="$2"; shift 2 ;;
        --prefix=*) prefix="${1#*=}"; shift ;;
        --destdir) [ $# -ge 2 ] || die "--destdir needs a directory"; destdir="$2"; shift 2 ;;
        --destdir=*) destdir="${1#*=}"; shift ;;
        --link-binary) [ $# -ge 2 ] || die "--link-binary needs a path"; link_binary="$2"; shift 2 ;;
        --link-binary=*) link_binary="${1#*=}"; shift ;;
        --uninstall) uninstall=1; shift ;;
        -h|--help) sed -n '2,50p' -- "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done

case "$prefix" in
    /*) ;;
    *) die "--prefix must be an absolute path (got '$prefix')" ;;
esac

apps_dir="$destdir$prefix/share/applications"
dbus_dir="$destdir$prefix/share/dbus-1/services"
icon_dir="$destdir$prefix/share/icons/hicolor/scalable/apps"
desktop_dst="$apps_dir/$APP_ID.desktop"
icon_dst="$icon_dir/$APP_ID.svg"
dbus_dst="$dbus_dir/$APP_ID.service"
bin_dst="$destdir$prefix/bin/omnibridge-gui"

# Refreshing the caches is for a live session only. In DESTDIR mode the
# package's own file triggers do it on the installing machine, and running it
# here would be writing a cache for a root that is not the one being used.
refresh_caches() {
    [ -n "$destdir" ] && return 0
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        # Not an error when the theme has no index: GTK and GNOME both fall
        # back to reading the directory, which is all a single SVG needs.
        gtk-update-icon-cache -qtf "$prefix/share/icons/hicolor" 2>/dev/null || true
    fi
    # The message bus does not watch its service directories. Measured on
    # Fedora 44, whose bus is dbus-broker: immediately after the service file
    # was written, activating the name still failed with
    # `ServiceUnknown: The name is not activatable`, and the identical call
    # succeeded the moment `ReloadConfig` had been sent. Without this line the
    # tray's cold-start path works only after the next login, which is exactly
    # the kind of "works on my machine tomorrow" this script exists to avoid.
    #
    # Harmless when there is no session bus — a build machine, a DESTDIR run
    # that got this far — so the failure is swallowed rather than reported.
    if command -v gdbus >/dev/null 2>&1; then
        gdbus call --session --dest org.freedesktop.DBus \
            --object-path /org/freedesktop/DBus \
            --method org.freedesktop.DBus.ReloadConfig >/dev/null 2>&1 || true
    fi
}

if [ "$uninstall" -eq 1 ]; then
    removed=0
    for f in "$desktop_dst" "$icon_dst" "$dbus_dst"; do
        if [ -e "$f" ]; then rm -f -- "$f"; printf 'removed  %s\n' "$f"; removed=1; fi
    done
    # Only ever a symlink this script made, never a real binary someone put
    # there.
    if [ -L "$bin_dst" ]; then rm -f -- "$bin_dst"; printf 'removed  %s\n' "$bin_dst"; removed=1; fi
    refresh_caches
    [ "$removed" -eq 1 ] || printf 'nothing to remove under %s\n' "$prefix"
    exit 0
fi

[ -f "$DESKTOP_SRC" ] || die "desktop entry not found at $DESKTOP_SRC"
[ -f "$ICON_SRC" ] || die "application icon not found at $ICON_SRC"
[ -f "$DBUS_SRC" ] || die "D-Bus activation template not found at $DBUS_SRC"

# Validated before it is installed rather than after: a malformed entry is
# ignored by the session silently, which looks exactly like this script not
# having run.
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_SRC" || die "the desktop entry does not validate"
fi

install -d -- "$apps_dir" "$icon_dir" "$dbus_dir"
# Verbatim, both of them. Rewriting either here is what would let a
# development run and a package disagree about the application's identity.
# The service file below is the one that is generated, and it is generated
# from a prefix rather than from anything about this checkout.
install -m 0644 -- "$DESKTOP_SRC" "$desktop_dst"
install -m 0644 -- "$ICON_SRC" "$icon_dst"
printf 'installed %s\n' "$desktop_dst"
printf 'installed %s\n' "$icon_dst"

# The one file that cannot be installed verbatim. `@BINDIR@` becomes the bin
# directory of the prefix that was selected — never `$destdir`, which is a
# staging root that does not exist on the machine that will read this file,
# and never a path from this checkout. `install` then writes it atomically.
sed -e "s|@BINDIR@|$prefix/bin|g" -- "$DBUS_SRC" > "$dbus_dst.tmp"
install -m 0644 -- "$dbus_dst.tmp" "$dbus_dst"
rm -f -- "$dbus_dst.tmp"
printf 'installed %s\n' "$dbus_dst"

if [ -n "$link_binary" ]; then
    [ -x "$link_binary" ] || die "not an executable: $link_binary"
    install -d -- "$destdir$prefix/bin"
    ln -sfn -- "$(cd -- "$(dirname -- "$link_binary")" && pwd)/$(basename -- "$link_binary")" "$bin_dst"
    printf 'linked    %s -> %s\n' "$bin_dst" "$link_binary"
    case ":$PATH:" in
        *":$prefix/bin:"*) ;;
        *) printf 'note      %s is not on your PATH; the launcher will not start it\n' "$prefix/bin" ;;
    esac
fi

refresh_caches

cat <<EOF

Desktop metadata installed under $prefix.

The D-Bus activation entry means the session bus can *start* OmniBridge when
something asks for one of its actions by name — which is what the KDE tray
item does when no GUI is running. The bus rereads this directory on demand,
so no restart is needed.

GNOME Shell matches a window to this entry by its Wayland app_id, which is
already $APP_ID — so a window opened from now on
resolves the OmniBridge icon. A window that was already open when this ran may
keep the generic one until it is reopened.
EOF
