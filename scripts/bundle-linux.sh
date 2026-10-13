#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Build the desktop version as a Linux tarball,
# target/release/ibisOffice-<version>-Linux-<arch>.tar.gz, and its SHA-256
# beside it (<tarball>.sha256, which the automatic update checks the download
# against). <version> is version.txt's Linux line; <arch> is what `uname -m`
# says (x86_64, aarch64).
#
#   scripts/bundle-linux.sh               build the executables, then package them
#   scripts/bundle-linux.sh --no-build    package the ones already in target/release
#
# The tarball holds one folder, ibisOffice/, that runs where it is unpacked
# (./ibisOffice/ibisOffice file.docx) and installs with its install.sh:
#
#   ibisOffice, ibisOfficeUpdater    the app and the automatic update's program, stripped
#   install.sh, uninstall.sh         crates/ibis-office-app/linux/
#   jp.ne.ibis.office.desktop        the entry in the menus (install.sh fills in Exec)
#   icons/hicolor/                   the app icon (made by scripts/gen-linux-icons.sh)
#   Resources/artborders/            the pictures of Word's art page borders, as in the macOS app
#   Resources/Licenses/              the third-party licences, as in the macOS app
#   LICENSE, NOTICE, ...             ibisOffice's own licence and notices (AGPL-3.0
#                                    has them go with the executables)
#
# UI icons and fonts are embedded in the executable.
#
# The automatic update asks ibis inc.'s repository, whose version.txt has no
# Linux line (so nothing is offered), unless the build names another:
#
#   IBIS_UPDATE_REPO=owner/repo scripts/bundle-linux.sh
#
# (and IBIS_UPDATE_BRANCH, if version.txt is not read from `main`; see
# crates/ibis-office-app/src/shell/update.rs). CARGO_BUILD_JOBS limits the
# build's jobs.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD=1
for arg in "$@"; do
    case $arg in
        --no-build) BUILD=0 ;;
        *)
            echo "usage: scripts/bundle-linux.sh [--no-build]" >&2
            exit 2
            ;;
    esac
done

# The version shown for Linux, from version.txt (a platform, a tab and its
# version, per line). It goes into a file name: digits and dots only.
VERSION=$(awk -F'\t' '$1 == "Linux" { sub(/\r$/, "", $2); print $2 }' version.txt)
[[ -n $VERSION ]] || { echo "version.txt has no Linux line" >&2; exit 1; }
[[ $VERSION =~ ^[0-9]+(\.[0-9]+)*$ ]] || { echo "version.txt: the Linux version is not digits and dots: $VERSION" >&2; exit 1; }
ARCH=$(uname -m)

if [[ $BUILD == 1 ]]; then
    # As scripts/bundle-macos.sh: the paths of this machine (the home folder,
    # the repository) are not to go into the executable, so they are rewritten
    # to short relative ones, in a target directory of its own so that the
    # everyday builds (target/release) are not rebuilt by the flags. The flags
    # are handed over with the separator cargo reads them back by, so that a
    # path with a space in it stays one flag.
    SEP=$'\x1f'
    CARGO_DIR=${CARGO_HOME:-$HOME/.cargo}
    RUSTUP_DIR=${RUSTUP_HOME:-$HOME/.rustup}
    REMAP="--remap-path-prefix=$CARGO_DIR/registry/src=crates.io${SEP}--remap-path-prefix=$CARGO_DIR/git/checkouts=git${SEP}--remap-path-prefix=$RUSTUP_DIR=rustup${SEP}--remap-path-prefix=$PWD=."
    # IBIS_UPDATE_REPO and IBIS_UPDATE_BRANCH, if set, are read by the build from this environment.
    CARGO_ENCODED_RUSTFLAGS="$REMAP" CARGO_TARGET_DIR=target/dist cargo build --release --locked -p ibis-office-app -p ibis-office-updater
    BIN=target/dist/release
    if [[ -n ${IBIS_UPDATE_REPO:-} ]]; then
        echo "The automatic update asks $IBIS_UPDATE_REPO (version.txt on ${IBIS_UPDATE_BRANCH:-main})"
    else
        echo "The automatic update asks ibisjp/ibis-office, which has no Linux versions to offer (IBIS_UPDATE_REPO names another)"
    fi
else
    BIN=target/release
    if [[ -n ${IBIS_UPDATE_REPO:-}${IBIS_UPDATE_BRANCH:-} ]]; then
        echo "warning: IBIS_UPDATE_REPO and IBIS_UPDATE_BRANCH are read when the executables are built: --no-build packages them as they are" >&2
    fi
fi
for program in ibisOffice ibisOfficeUpdater; do
    [[ -x $BIN/$program ]] || { echo "$BIN/$program is not there: build it (cargo build --release -p ibis-office-app -p ibis-office-updater), or run this without --no-build" >&2; exit 1; }
done

mkdir -p target/release
STAGE=$(mktemp -d target/release/bundle-linux.XXXXXX)
trap 'rm -rf "$STAGE"' EXIT
APP="$STAGE/ibisOffice"
mkdir -p "$APP"

# The executables, without their symbol tables.
cp "$BIN/ibisOffice" "$BIN/ibisOfficeUpdater" "$APP/"
strip "$APP/ibisOffice" "$APP/ibisOfficeUpdater"
chmod 755 "$APP/ibisOffice" "$APP/ibisOfficeUpdater"

# Installing: the scripts, the menu entry and the icons.
LINUX=crates/ibis-office-app/linux
cp "$LINUX/install.sh" "$LINUX/uninstall.sh" "$APP/"
chmod 755 "$APP/install.sh" "$APP/uninstall.sh"
cp "$LINUX/jp.ne.ibis.office.desktop" "$APP/"
chmod 644 "$APP/jp.ne.ibis.office.desktop"
if command -v desktop-file-validate > /dev/null; then
    desktop-file-validate "$APP/jp.ne.ibis.office.desktop"
fi
mkdir -p "$APP/icons"
cp -R "$LINUX/icons/hicolor" "$APP/icons/hicolor"

# The third-party components in the app and the notices their licences ask
# for (made afresh from the dependencies), and Carlito's licence on its own
# (OFL-1.1 asks for it with the font).
python3 scripts/gen-web-licenses.py linux
mkdir -p "$APP/Resources/Licenses"
cp "$LINUX/license.md" "$APP/Resources/Licenses/license.md"
cp crates/ibis-office-app/fonts/carlito/OFL.txt "$APP/Resources/Licenses/Carlito-OFL.txt"
# The pictures of Word's art page borders: ECMA-376's zip, unmodified, with
# its notice (read when a document uses an art border; not in the executable).
# The app looks for Resources/artborders/ beside its executable.
mkdir -p "$APP/Resources/artborders"
cp crates/ibis-office-app/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip crates/ibis-office-app/data/artborders/ECMA-NOTICE.txt "$APP/Resources/artborders/"
# ibisOffice's own licence (AGPL-3.0: a copy goes with the executables), its
# notice (the additional terms) and the files those two send the reader to.
cp LICENSE NOTICE TRADEMARK.md COMMERCIAL-LICENSE.md THIRD-PARTY-NOTICES.md "$APP/"

# The tarball: files in the order of their names, owned by nobody in
# particular, so that the same files make the same list whoever packs them.
NAME="ibisOffice-$VERSION-Linux-$ARCH.tar.gz"
OUT="target/release/$NAME"
tar --create --gzip --file "$OUT" --directory "$STAGE" --sort=name --owner=0 --group=0 --numeric-owner ibisOffice
# `<hash>  <file name>`, as `sha256sum --check` reads it and as the automatic
# update reads the Windows zip's.
(cd target/release && sha256sum "$NAME" > "$NAME.sha256")

echo "Built $OUT ($(du -h "$OUT" | cut -f1)) and $OUT.sha256"
echo "Try it: tar -xzf $OUT -C <folder> && <folder>/ibisOffice/install.sh"
