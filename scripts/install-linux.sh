#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
# Download the newest Linux release of ibisOffice and install it:
#
#   curl -fsSL https://raw.githubusercontent.com/lunagoat/ibis-office/HEAD/scripts/install-linux.sh | sh
#
# The options are those of the tarball's install.sh
# (crates/ibis-office-app/linux/install.sh). Without any, ibisOffice is
# installed for this user in ~/.local, and root is not needed. For every user:
#
#   curl -fsSL <the same address> | sudo sh -s -- --prefix /usr/local
#
# The newest release is the one GitHub's releases/latest page sends on to
# (tagged v<version>). Its ibisOffice-<version>-Linux-<arch>.tar.gz and the
# .sha256 beside it are downloaded to a temporary folder, the tarball is
# checked against the hash and unpacked there, and its install.sh is run.
# Running this again installs the newest release over the one installed.
#
# IBIS_UPDATE_REPO=owner/repo asks another repository. For the tests,
# IBIS_UPDATE_URL_BASE=<address> takes GitHub's place, as it does for the
# automatic update: <address>/version.txt names the version (its Linux line)
# and the two files are beside it.
set -eu

REPO=${IBIS_UPDATE_REPO:-lunagoat/ibis-office}

die() {
    printf 'install-linux.sh: %s\n' "$*" >&2
    exit 1
}

# curl, quiet but for its errors. GitHub is asked over https only, also where
# it sends on to; the tests' address may be a file.
fetch() {
    if [ -n "${IBIS_UPDATE_URL_BASE:-}" ]; then
        curl --fail --silent --show-error --location --connect-timeout 10 "$@"
    else
        curl --fail --silent --show-error --location --connect-timeout 10 --proto '=https' --proto-redir '=https' "$@"
    fi
}

# All of it in one function, called on the last line: a download of this
# script that stops half way then runs nothing.
main() {
    [ "$(uname -s)" = Linux ] || die 'this installs the Linux build of ibisOffice (the README says where the others are)'
    for tool in curl tar sha256sum mktemp; do
        command -v "$tool" > /dev/null 2>&1 || die "$tool is not installed"
    done
    arch=$(uname -m)

    base=${IBIS_UPDATE_URL_BASE:-}
    if [ -n "$base" ]; then
        base=${base%/}
        version=$(fetch "$base/version.txt" | awk -F'\t' '$1 == "Linux" { sub(/\r$/, "", $2); print $2 }')
    else
        latest=$(fetch --output /dev/null --write-out '%{url_effective}' "https://github.com/$REPO/releases/latest") \
            || die "cannot read https://github.com/$REPO/releases/latest"
        case $latest in
            */releases/tag/v*) version=${latest##*/releases/tag/v} ;;
            *) die "https://github.com/$REPO has no release yet" ;;
        esac
        base=https://github.com/$REPO/releases/download/v$version
    fi
    # Digits and dots only: it goes into a file name and an address.
    case $version in
        '' | .* | *. | *..* | *[!0-9.]*) die "cannot tell the newest version (got '$version')" ;;
    esac

    name=ibisOffice-$version-Linux-$arch.tar.gz
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/ibisOffice-install.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 1' HUP INT TERM

    printf 'Downloading ibisOffice %s for %s\n' "$version" "$arch"
    fetch --output "$tmp/$name" "$base/$name" \
        || die "$base/$name could not be downloaded (release $version may have no build for $arch: the README says how to build one)"
    fetch --output "$tmp/$name.sha256" "$base/$name.sha256" || die "$base/$name.sha256 could not be downloaded"

    # `<hash>  <file name>`, or the hash alone: what the automatic update reads too.
    want=$(sed -n '1s/^\([0-9A-Fa-f]\{64\}\).*/\1/p' "$tmp/$name.sha256" | tr 'A-F' 'a-f')
    got=$(sha256sum < "$tmp/$name" | cut -c1-64)
    [ -n "$want" ] && [ "$want" = "$got" ] || die "$name does not match its SHA-256: nothing was installed"

    tar -xzf "$tmp/$name" -C "$tmp"
    [ -f "$tmp/ibisOffice/install.sh" ] || die "$name has no ibisOffice/install.sh in it"
    sh "$tmp/ibisOffice/install.sh" "$@"
}

main "$@"
