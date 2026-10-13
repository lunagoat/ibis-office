#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Remove the ibisOffice that install.sh installed. Run the one in the
# installed folder (install.sh says where: <prefix>/lib/ibisOffice/uninstall.sh),
# as root if it was installed as root.
#
# It removes what install.sh noted in <prefix>/lib/ibisOffice.install-manifest
# (the link, the menu entry, the icons, and the folders it had to make, if
# nothing else is in them now), the installed folder and that note. Nothing
# else: the user's settings and own files stay where they are (it says where).
set -eu

APP_ID=jp.ne.ibis.office

die() {
    printf 'uninstall.sh: %s\n' "$*" >&2
    exit 1
}

warn() {
    printf 'uninstall.sh: warning: %s\n' "$*" >&2
}

case ${1:-} in
    '') ;;
    -h | --help)
        printf '%s\n' 'Usage: uninstall.sh' '' 'Removes the ibisOffice that install.sh installed in the folder this script is in.'
        exit 0
        ;;
    *) die "unknown option: $1 (see --help)" ;;
esac

app_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P) || die 'cannot find the folder of this script'
manifest=$app_dir.install-manifest
[ -f "$app_dir/ibisOffice" ] || die "$app_dir is not a folder of ibisOffice"
[ -f "$manifest" ] || die "$manifest is not there: this copy was not put here by install.sh, and nothing was removed. A copy that was only unpacked is removed by deleting its folder."
[ -w "$manifest" ] && [ -w "$(dirname -- "$app_dir")" ] || die "cannot write to $(dirname -- "$app_dir"): run this as the user who installed it (root, for a copy installed for every user)"

# The link and the files, each only if it is still what install.sh put there.
desktop_dirs=
icon_themes=
while IFS= read -r entry; do
    case $entry in
        link:*)
            path=${entry#link:}
            [ -h "$path" ] || continue
            target=$(readlink -- "$path" 2> /dev/null) || target=
            if [ "$target" = "$app_dir/ibisOffice" ]; then
                rm -f -- "$path"
            else
                warn "$path no longer points to this copy: left as it is"
            fi
            ;;
        file:*)
            path=${entry#file:}
            if [ -f "$path" ] && [ ! -h "$path" ]; then
                rm -f -- "$path"
            fi
            # The lists the desktop keeps of these folders are made again below.
            case $path in
                */applications/"$APP_ID".desktop)
                    desktop_dirs="$desktop_dirs${path%/*}
"
                    ;;
                */icons/hicolor/*/apps/"$APP_ID".png)
                    icon_themes="$icon_themes${path%/*/apps/*}
"
                    ;;
            esac
            ;;
    esac
done < "$manifest"

# The folders install.sh had to make, the deepest first (sorted backwards: a
# folder's name comes after the names of the folders in it), each removed
# below only if it is empty.
dirs=$(sed -n 's/^dir://p' "$manifest" | LC_ALL=C sort -r)

# Not from inside the folder being removed.
cd /
rm -rf -- "$app_dir"
rm -f -- "$manifest"

printf '%s' "$desktop_dirs" | sort -u | while IFS= read -r dir; do
    [ -d "$dir" ] || continue
    if command -v update-desktop-database > /dev/null 2>&1; then
        update-desktop-database "$dir" > /dev/null 2>&1 || true
    fi
done
printf '%s' "$icon_themes" | sort -u | while IFS= read -r dir; do
    [ -d "$dir" ] || continue
    if command -v gtk-update-icon-cache > /dev/null 2>&1; then
        gtk-update-icon-cache -q -t -f "$dir" > /dev/null 2>&1 || true
    fi
done

# Whether `$1` holds nothing but the file named `$2`.
holds_only() {
    for inside in "$1"/* "$1"/.[!.]* "$1"/..?*; do
        if [ -e "$inside" ] || [ -h "$inside" ]; then
            [ "$inside" = "$1/$2" ] || return 1
        fi
    done
    [ -f "$1/$2" ]
}

printf '%s\n' "$dirs" | while IFS= read -r dir; do
    [ -n "$dir" ] && [ -d "$dir" ] && [ ! -h "$dir" ] || continue
    # A list of a folder with nothing left in it, written by the tools above
    # (or when installing), lists nothing: it goes with the folder.
    case $dir in
        */applications) cache=mimeinfo.cache ;;
        */icons/hicolor) cache=icon-theme.cache ;;
        *) cache= ;;
    esac
    if [ -n "$cache" ] && holds_only "$dir" "$cache"; then
        rm -f -- "$dir/$cache"
    fi
    rmdir -- "$dir" 2> /dev/null || true
done

printf '%s\n' "ibisOffice is removed from $app_dir"

# What was never install.sh's to remove.
kept=
for dir in "${XDG_CONFIG_HOME:-${HOME:-}/.config}/ibisOffice" "${XDG_DATA_HOME:-${HOME:-}/.local/share}/ibisOffice" "${XDG_CACHE_HOME:-${HOME:-}/.cache}/ibisOffice" "${XDG_STATE_HOME:-${HOME:-}/.local/state}/ibisOffice"; do
    if [ -d "$dir" ]; then
        kept="$kept
  $dir"
    fi
done
if [ -n "$kept" ]; then
    printf '%s\n' "Your settings and your own files (recent files, AutoRecover, chart templates, the update log) are kept:$kept"
fi
