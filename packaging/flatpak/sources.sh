#!/bin/sh
# Export the committed application and every checksummed build input.
set -eu
cd "$(dirname "$0")/../.."
git diff --quiet HEAD -- || { echo 'Commit source changes before exporting release sources.' >&2; exit 1; }
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
stage="outputs/flatpak-source/ondrane-$version-flatpak-source"
[ -d .flatpak-builder/downloads ] || { echo 'Build the Flatpak first.' >&2; exit 1; }
mkdir -p "$stage" target
git archive HEAD | tar -xf - -C "$stage"
mkdir -p "$stage/.flatpak-builder"
cp -R .flatpak-builder/downloads "$stage/.flatpak-builder/"
tar -czf "target/ondrane-$version-flatpak-source.tar.gz" -C outputs/flatpak-source "ondrane-$version-flatpak-source"
