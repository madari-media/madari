#!/usr/bin/env bash
# Vendors prebuilt libmpv XCFrameworks for iOS into apps/ios/native/mpv
# (gitignored, like libmadari_ios.a).
#
# Why a download instead of a source build: libmpv drags in FFmpeg, libass,
# freetype, harfbuzz, fribidi and dav1d, and cross-building that stack from Linux
# is a multi-day project. media-kit/libmpv-darwin-build publishes exactly the
# artifact we need, so we vendor it ourselves, pin its checksum, and link it with
# SwiftPM binary targets. Nothing here is a package dependency: SwiftPM never
# resolves or fetches anything for this, scripts/fetch-ios-mpv.sh does.
#
# The build is libmpv-only, LGPL (no GPL codecs, no encoders) and configured with
# the iOS backends we rely on:
#   -Dcplayer=false -Dlibmpv=true -Dgpl=false
#   -Daudiounit=enabled   AudioUnit output, so iOS audio works
#   -Dios-gl=enabled -Dgl=enabled -Dplain-gl=enabled
#                         VideoToolbox -> OpenGL ES interop and the libmpv
#                         OpenGL render API, which is how we present frames
#   -Dlibplacebo=disabled -Dvulkan=disabled -Dvideotoolbox-gl=disabled
#                         no Metal/Vulkan path is involved
#
# See docs/ios.md for the licensing note that comes with shipping these.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${MADARI_MPV_VERSION:-v0.7.2}"
variant="${MADARI_MPV_VARIANT:-ios-universal-video-default}"
# sha256 of the release asset this script downloads.
sha256="${MADARI_MPV_SHA256:-a0dbcddc0eaefa5534eb2bdc797e5386b1e0cd4057ed8f73aa2dd6105503dffb}"

dist="libmpv-xcframeworks_${version}_${variant}"
url="https://github.com/media-kit/libmpv-darwin-build/releases/download/${version}/${dist}.tar.gz"
cache="build/cache/${dist}.tar.gz"
dest="apps/ios/native/mpv"

mkdir -p build/cache
if [[ ! -f "$cache" ]]; then
  echo "==> Downloading libmpv $version ($variant)"
  curl -fL --retry 3 -o "$cache.part" "$url"
  mv "$cache.part" "$cache"
fi

echo "==> Verifying $cache"
echo "$sha256  $cache" | sha256sum -c - >/dev/null

echo "==> Extracting into $dest"
rm -rf "$dest"
mkdir -p "$dest"
tar -xzf "$cache" -C "$dest" --strip-components=1

# Provenance and licensing, kept next to the binaries so it travels with them.
cat >"$dest/SOURCE.txt" <<EOF
libmpv XCFrameworks for iOS, vendored by scripts/fetch-ios-mpv.sh.

source:  $url
version: $version ($variant)
sha256:  $sha256
contents: mpv 0.36.0, FFmpeg 6.0, libass, freetype, harfbuzz, fribidi, dav1d,
          mbedtls, libxml2, uchardet, libpng.

These are dynamic frameworks and are embedded in the app bundle, so the LGPL
obligation to allow replacement is met by the dynamic link itself. mpv and
FFmpeg are LGPL-2.1-or-later; the build has no GPL components (-Dgpl=false).
EOF

echo "==> Vendored frameworks:"
ls "$dest" | grep '\.xcframework$' | sed 's/^/    /'
echo
echo "libmpv $version -> $dest"
