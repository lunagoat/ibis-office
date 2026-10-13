#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Install ibisOffice from the folder this script is in (the unpacked tarball).
#
#   ./install.sh                        for this user; root is not needed
#   ./install.sh --prefix /usr/local    for every user (run it as root)
#
# What goes where; <prefix> is ~/.local without --prefix, and <data> is then
# $XDG_DATA_HOME (~/.local/share if that is not set), else <prefix>/share:
#
#   <prefix>/lib/ibisOffice/                          this folder, copied
#   <prefix>/bin/ibisOffice                           a link to the program in it
#   <data>/applications/jp.ne.ibis.office.desktop     the entry in the menus
#   <data>/icons/hicolor/<size>/apps/jp.ne.ibis.office.png
#   <prefix>/lib/ibisOffice.install-manifest          what was put where, for uninstall.sh
#
# The folder is not put in <data>/ibisOffice: ibisOffice keeps the user's own
# files there (recent files, AutoRecover, chart templates), and the folder of
# the app is replaced whole, by installing again and by the automatic update.
#
# Installing again replaces the copy installed before. A copy installed for
# this user keeps itself up to date; one installed with --prefix is kept up to
# date by whoever installed it. To remove it: uninstall.sh in the installed folder.
set -eu

APP_ID=jp.ne.ibis.office

die() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}

warn() {
    printf 'install.sh: warning: %s\n' "$*" >&2
}

usage() {
    printf '%s\n' \
        'Usage: install.sh [--prefix <dir>]' \
        '' \
        'Without --prefix, ibisOffice is installed for this user in ~/.local' \
        '(root is not needed). With --prefix, for every user in <dir>, for' \
        'example /usr/local (run it as root).'
}

prefix=
system=
while [ $# -gt 0 ]; do
    case $1 in
        --prefix)
            [ $# -ge 2 ] || die '--prefix needs a folder'
            prefix=$2
            system=1
            shift 2
            ;;
        --prefix=*)
            prefix=${1#--prefix=}
            system=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown option: $1 (see --help)"
            ;;
    esac
done

# The folder this script is in, whatever folder it was started from.
src=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P) || die 'cannot find the folder of this script'
for program in ibisOffice ibisOfficeUpdater; do
    [ -f "$src/$program" ] && [ -x "$src/$program" ] || die "$src/$program is missing: run install.sh from the unpacked ibisOffice folder"
done
[ -f "$src/$APP_ID.desktop" ] || die "$src/$APP_ID.desktop is missing"

if [ -n "$system" ]; then
    [ -n "$prefix" ] || die '--prefix needs a folder'
    case $prefix in
        /*) ;;
        *) prefix=$(pwd)/$prefix ;;
    esac
    data=$prefix/share
    # Whatever root's umask is, every user must be able to read and run it.
    umask 022
else
    [ -n "${HOME:-}" ] || die 'HOME is not set'
    prefix=$HOME/.local
    data=${XDG_DATA_HOME:-}
    # As the XDG specification has it: a value that is not an absolute path is ignored.
    case $data in
        /*) ;;
        *) data=$HOME/.local/share ;;
    esac
fi
# No slash at the end.
while [ "$prefix" != / ] && [ "${prefix%/}" != "$prefix" ]; do prefix=${prefix%/}; done
while [ "$data" != / ] && [ "${data%/}" != "$data" ]; do data=${data%/}; done
case $prefix$data in
    *'
'*) die 'a folder with a line break in its name cannot be installed into' ;;
esac

lib_dir=$prefix/lib
bin_dir=$prefix/bin
app_dir=$lib_dir/ibisOffice
manifest=$lib_dir/ibisOffice.install-manifest
staged=$app_dir.install-new
records=$manifest.new

# Make a folder and the missing ones above it, noting each one made:
# uninstall.sh removes those again, and no folder that was there before.
made_dirs=
make_dir() {
    [ -d "$1" ] && return 0
    make_dir "$(dirname -- "$1")"
    mkdir -- "$1" || die "cannot make $1 (to install for every user, run this as root)"
    made_dirs="$made_dirs$1
"
}

make_dir "$lib_dir"
[ -w "$lib_dir" ] || die "cannot write to $lib_dir (to install for every user, run this as root)"
make_dir "$bin_dir"
make_dir "$data/applications"
rm -f -- "$records"

# The app folder: a copy made beside the place it goes, then put there in one
# step, so that a copy that fails half way leaves what was installed before.
# (Run from the installed folder itself, there is nothing to copy: the link,
# the menu entry and the icons are put back.)
if [ -d "$app_dir" ] && [ "$(CDPATH='' cd -- "$app_dir" && pwd -P)" = "$src" ]; then
    :
else
    rm -rf -- "$staged"
    mkdir -- "$staged"
    cp -R -P -- "$src/." "$staged/"
    if [ -n "$system" ]; then
        chmod -R a+rX,go-w -- "$staged"
    fi
    rm -rf -- "$app_dir"
    mv -- "$staged" "$app_dir"
fi
# The path with no link in it: what the program finds itself by, and what
# uninstall.sh (which looks from the installed folder) compares with.
app_dir=$(CDPATH='' cd -- "$app_dir" && pwd -P)

# The launcher: a link, so that the program is still found inside its folder
# (where its updater and its Resources are).
link=$bin_dir/ibisOffice
if [ -e "$link" ] || [ -h "$link" ]; then
    [ -h "$link" ] || die "$link is there already and is not a link: move it away first"
    rm -f -- "$link"
fi
ln -s -- "$app_dir/ibisOffice" "$link"
printf 'link:%s\n' "$link" >> "$records"

# The path of the program as a .desktop file's Exec line has it. A path of
# plain characters is written as it is. Any other goes in double quotes, where
# \ " ` and $ take a backslash; every backslash is then doubled once more, as
# in any text of a .desktop file; and % is doubled (one % starts a field code
# such as %f).
desktop_exec() {
    case $1 in
        *[!A-Za-z0-9_./+@:,-]*)
            printf '"%s"' "$(printf '%s\n' "$1" | sed -e 's/[\\"`$]/\\&/g' -e 's/\\/\\\\/g' -e 's/%/%%/g')"
            ;;
        *)
            printf '%s' "$1"
            ;;
    esac
}

# The menu entry: the tarball's, with the place of the program filled in.
desktop=$data/applications/$APP_ID.desktop
exec_line="Exec=$(desktop_exec "$app_dir/ibisOffice") %f"
while IFS= read -r line || [ -n "$line" ]; do
    case $line in
        Exec=*) printf '%s\n' "$exec_line" ;;
        *) printf '%s\n' "$line" ;;
    esac
done < "$src/$APP_ID.desktop" > "$desktop.new"
chmod 644 -- "$desktop.new"
mv -f -- "$desktop.new" "$desktop"
printf 'file:%s\n' "$desktop" >> "$records"

# The icons, in the hicolor theme (the one every other theme falls back to).
for from in "$src"/icons/hicolor/*/apps/"$APP_ID".png; do
    [ -f "$from" ] || continue
    size=${from%/apps/*}
    size=${size##*/}
    icon=$data/icons/hicolor/$size/apps/$APP_ID.png
    make_dir "$data/icons/hicolor/$size/apps"
    cp -- "$from" "$icon"
    chmod 644 -- "$icon"
    printf 'file:%s\n' "$icon" >> "$records"
done

# What uninstall.sh removes: what this install put there, and what an install
# before it had noted (a folder that one made is still one to remove).
printf '%s' "$made_dirs" | while IFS= read -r dir; do
    printf 'dir:%s\n' "$dir"
done >> "$records"
if [ -f "$manifest" ]; then
    cat -- "$manifest" >> "$records"
fi
sort -u -- "$records" > "$manifest"
rm -f -- "$records"

# The desktop's lists of entries and icons, where their tools are installed.
# Without them the desktop reads the folders itself, and a tool that fails is
# not a failed install.
if command -v update-desktop-database > /dev/null 2>&1; then
    update-desktop-database "$data/applications" > /dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache > /dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f "$data/icons/hicolor" > /dev/null 2>&1 || true
fi

printf '%s\n' "ibisOffice is installed in $app_dir"
printf '%s\n' 'Start it from the applications menu, or with: ibisOffice'
printf '%s\n' "To remove it: $app_dir/uninstall.sh"

case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *) warn "$bin_dir is not on your PATH: start ibisOffice from the menu or as $link, or add that folder to PATH" ;;
esac
if [ -z "$system" ]; then
    for tool in curl tar sha256sum; do
        command -v "$tool" > /dev/null 2>&1 || warn "$tool is not installed: ibisOffice needs it to update itself"
    done
fi
