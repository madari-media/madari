#!/usr/bin/env bash
# Builds the AppImage for the current architecture.
#
# The bundle carries the application, its desktop entry and its icons, and runs
# against the system GTK4, libadwaita and mpv — the same libraries the .deb and
# .rpm require. It does not vendor those libraries: bundling GTK4 correctly needs
# linuxdeploy and its GTK plugin, and a half-bundled GTK is worse than none. This
# produces a single runnable file with desktop integration, not a portable
# runtime for a distro that lacks the dependencies (which must be pango >= 1.56
# or newer regardless, because the binary calls pango 1.56 APIs).
set -euo pipefail
cd "$(dirname "$0")/.."

version="${1:-$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -1)}"
binary="target/release/madari"
desktop="apps/linux/assets/io.github.madari.Madari.desktop"
icons="apps/linux/assets/icons"
appdir="target/appimage/Madari.AppDir"

if [[ ! -x "$binary" ]]; then
  echo "Missing $binary. Run 'cargo build --locked --release -p madari-linux' first." >&2
  exit 1
fi

case "$(uname -m)" in
  x86_64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *)
    echo "Unsupported architecture $(uname -m) for the AppImage." >&2
    exit 1
    ;;
esac

rm -rf "$appdir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications"
install -m 755 "$binary" "$appdir/usr/bin/madari"
install -m 644 "$desktop" "$appdir/usr/share/applications/"
install -m 644 "$desktop" "$appdir/io.github.madari.Madari.desktop"

for size in 16 24 32 48 64 128 256 512; do
  dir="$appdir/usr/share/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  install -m 644 "$icons/${size}x${size}.png" "$dir/io.github.madari.Madari.png"
done
# appimagetool wants the icon at the AppDir root, and .DirIcon for the thumbnail.
install -m 644 "$icons/256x256.png" "$appdir/io.github.madari.Madari.png"
install -m 644 "$icons/256x256.png" "$appdir/.DirIcon"

cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
# Resolve through symlinks so the launcher works when the AppImage is on PATH.
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/madari" "$@"
APPRUN
chmod 755 "$appdir/AppRun"

# appimagetool ships as an AppImage, which needs FUSE to self-mount; extracting
# and running is what works in a container.
tool="target/appimage/appimagetool-$arch.AppImage"
mkdir -p target/appimage
if [[ ! -x "$tool" ]]; then
  curl -fsSL -o "$tool" \
    "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$arch.AppImage"
  chmod +x "$tool"
fi

export ARCH="$arch"
export APPIMAGE_EXTRACT_AND_RUN=1
out="target/appimage/Madari-${version}-${arch}.AppImage"
"$tool" --no-appstream "$appdir" "$out"
echo "$out"
