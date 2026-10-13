#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the Linux app icons, crates/ibis-office-app/linux/icons/hicolor/<size>x<size>/apps/jp.ne.ibis.office.png.
#
#   scripts/gen-linux-icons.sh
#
# The icon is the app icon of the other platforms (the mark on its plate), not
# the wordmark in logo/. The sizes drawn for Windows (windows/png/ico: 16, 24,
# 32, 48 and 256 px, the small ones drawn to stay clear at their size) are
# copied as they are; 64, 128 and 512 px are macos/AppIcon.png, the 1024 px
# master, made smaller with ImageMagick (`magick`).
#
# The PNGs are committed (scripts/bundle-linux.sh only copies them), so this is
# run again only when the icon changes. They are laid out as the hicolor icon
# theme has them, under the name of the app's .desktop file.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=crates/ibis-office-app
OUT="$SRC/linux/icons/hicolor"
NAME=jp.ne.ibis.office.png

for size in 16 24 32 48 256; do
    mkdir -p "$OUT/${size}x${size}/apps"
    cp "$SRC/windows/png/ico/ibisOffice_icon_win_${size}px.png" "$OUT/${size}x${size}/apps/$NAME"
done
for size in 64 128 512; do
    mkdir -p "$OUT/${size}x${size}/apps"
    # -strip: no date in the file, so that the same picture is the same file.
    magick "$SRC/macos/AppIcon.png" -filter Lanczos -resize "${size}x${size}" -strip "$OUT/${size}x${size}/apps/$NAME"
done

echo "Wrote $OUT"
