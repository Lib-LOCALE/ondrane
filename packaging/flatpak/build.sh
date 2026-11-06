#!/bin/sh
# Build the upstream GitHub Flatpak; this is not a Flathub submission manifest.
set -eu
cd "$(dirname "$0")/../.."
flatpak_user_dir="${FLATPAK_USER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/flatpak}"
if command -v flatpak-builder >/dev/null 2>&1; then
  set -- flatpak-builder
else
  set -- flatpak run --env="FLATPAK_USER_DIR=$flatpak_user_dir" \
    --command=flatpak-builder org.flatpak.Builder
fi
"$@" --user --jobs="${CARGO_BUILD_JOBS:-8}" --force-clean \
  --repo=outputs/flatpak-repo outputs/flatpak-build \
  packaging/flatpak/io.github.lib_locale.ondrane.json
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p target
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  outputs/flatpak-repo "target/ondrane-$version-$(flatpak --default-arch).flatpak" \
  io.github.lib_locale.ondrane
