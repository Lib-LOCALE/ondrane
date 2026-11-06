#!/bin/sh
# Export the committed application and every checksummed build input.
set -eu
cd "$(dirname "$0")/../.."
git diff --quiet HEAD -- || { echo 'Commit source changes before exporting release sources.' >&2; exit 1; }
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
stage_root="$(mktemp -d outputs/flatpak-source.XXXXXX)"
trap 'rm -rf "$stage_root"' EXIT HUP INT TERM
stage="$stage_root/ondrane-$version-flatpak-source"
[ -d .flatpak-builder/downloads ] || { echo 'Build the Flatpak first.' >&2; exit 1; }
mkdir -p "$stage" target
git archive HEAD | tar -xf - -C "$stage"
mkdir -p "$stage/.flatpak-builder"
cp -R .flatpak-builder/downloads "$stage/.flatpak-builder/"
epoch="$(git show -s --format=%ct HEAD)"
tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
  -czf "target/ondrane-$version-flatpak-source.tar.gz" \
  -C "$stage_root" "ondrane-$version-flatpak-source"
