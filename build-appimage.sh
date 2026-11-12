#!/usr/bin/env bash
# Builds Oxide as an AppImage for Linux (x86_64).
#
# Usage:   ./build-appimage.sh
# Needs:   Rust (cargo), curl. Everything else is downloaded when missing.
# Output:  dist/Oxide-<version>-x86_64.AppImage
#
# Tip: build on the OLDEST Linux you want to support (e.g. Ubuntu 22.04): the
# AppImage only runs on systems with the same or a newer glibc.
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")"

NAME="Oxide"
CRATE="$(grep -m1 '^name' Cargo.toml | cut -d'"' -f2)"   # binary name = package name (e.g. "Oxide")
BIN="oxide"                                                  # name used inside the AppImage
TARGET="${CARGO_TARGET_DIR:-target}"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARCH="x86_64"
OUT="dist/${NAME}-${VERSION}-${ARCH}.AppImage"
TOOL="dist/appimagetool"
APPDIR="dist/${NAME}.AppDir"

echo "==> Building ${NAME} ${VERSION} (release)"
cargo build --release

echo "==> Preparing the AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
cp "${TARGET}/release/${CRATE}" "$APPDIR/usr/bin/${BIN}"
strip "$APPDIR/usr/bin/${BIN}" || true
cp assets/oxide.png "$APPDIR/oxide.png"
cp assets/oxide.png "$APPDIR/.DirIcon"
cp assets/oxide.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/oxide.png"

cat > "$APPDIR/oxide.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=${NAME}
GenericName=HTTP Client
Comment=A lightweight HTTP client
Exec=${BIN}
Icon=oxide
Categories=Development;Network;
Terminal=false
StartupWMClass=${BIN}
DESKTOP

cat > "$APPDIR/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/oxide" "$@"
APPRUN
chmod +x "$APPDIR/AppRun"

if [ ! -x "$TOOL" ]; then
    echo "==> Downloading appimagetool"
    curl -fsSL -o "$TOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
    chmod +x "$TOOL"
fi

echo "==> Creating ${OUT}"
rm -f "$OUT"
# --appimage-extract-and-run lets appimagetool work even where FUSE is missing (containers, CI).
ARCH="$ARCH" "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT"

echo
echo "Done: ${OUT}"
