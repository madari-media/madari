#!/usr/bin/env bash
# Vendors prebuilt libmpv XCFrameworks into the app trees (gitignored, like
# libmadari_ios.a):
#
#   ios    -> apps/ios/native/mpv
#   macos  -> apps/macos/native/mpv
#
# Despite the name this now covers both platforms. The name is kept because CI and
# docs/ios.md refer to it by that path.
#
# Why a download instead of a source build: libmpv drags in FFmpeg, libass,
# freetype, harfbuzz, fribidi and dav1d, and cross-building that stack from Linux
# is a multi-day project. media-kit/libmpv-darwin-build publishes exactly the
# artifact we need for both platforms, so we vendor it ourselves, pin each
# checksum, and link it with SwiftPM binary targets. Nothing here is a package
# dependency: SwiftPM never resolves or fetches anything for this, this script
# does.
#
# The iOS build is libmpv-only, LGPL (no GPL codecs, no encoders) and configured
# with the iOS backends we rely on:
#   -Dcplayer=false -Dlibmpv=true -Dgpl=false
#   -Daudiounit=enabled   AudioUnit output, so iOS audio works
#   -Dios-gl=enabled -Dgl=enabled -Dplain-gl=enabled
#                         VideoToolbox -> OpenGL ES interop and the libmpv
#                         OpenGL render API, which is how we present frames
#   -Dlibplacebo=disabled -Dvulkan=disabled -Dvideotoolbox-gl=disabled
#                         no Metal/Vulkan path is involved
#
# The macOS asset comes from the same release and the same build family, and its
# XCFramework carries a universal macos-arm64_x86_64 slice. The player surface for
# it is desktop OpenGL (NSOpenGLView), not OpenGL ES.
#
# See docs/ios.md for the licensing note that comes with shipping these.
set -euo pipefail
cd "$(dirname "$0")/.."

version="${MADARI_MPV_VERSION:-v0.7.2}"

# Which platforms to vendor. iOS is the default so that existing builds, and the CI
# ipa job that only needs iOS, are unaffected. A macOS build asks for macos or both:
#   MADARI_MPV_PLATFORMS="ios macos" bash scripts/fetch-ios-mpv.sh
platforms="${MADARI_MPV_PLATFORMS:-ios}"

# Per platform: the variant name in the release, and the sha256 of that asset.
ios_variant="${MADARI_MPV_VARIANT:-ios-universal-video-default}"
ios_sha256="${MADARI_MPV_SHA256:-a0dbcddc0eaefa5534eb2bdc797e5386b1e0cd4057ed8f73aa2dd6105503dffb}"
macos_variant="${MADARI_MPV_MACOS_VARIANT:-macos-universal-video-default}"
macos_sha256="${MADARI_MPV_MACOS_SHA256:-dd9928fff9c97329e17f69fe8ef0d621cf458f9f70847955f84b4eb1e9047b09}"

fetch() {
  local platform="$1" variant="$2" sha256="$3" dest="$4"
  local dist="libmpv-xcframeworks_${version}_${variant}"
  local url="https://github.com/media-kit/libmpv-darwin-build/releases/download/${version}/${dist}.tar.gz"
  local cache="build/cache/${dist}.tar.gz"

  mkdir -p build/cache
  if [[ ! -f "$cache" ]]; then
    echo "==> Downloading libmpv $version ($variant)"
    curl -fL --retry 3 -o "$cache.part" "$url"
    mv "$cache.part" "$cache"
  fi

  echo "==> Verifying $cache"
  # macOS ships shasum rather than sha256sum, and this runs on both: the development
  # machine is Linux, CI builds the same artifact on a macOS runner.
  if command -v sha256sum >/dev/null 2>&1; then
    echo "$sha256  $cache" | sha256sum -c - >/dev/null
  else
    local actual
    actual=$(shasum -a 256 "$cache" | awk '{print $1}')
    if [[ "$actual" != "$sha256" ]]; then
      echo "checksum mismatch for $cache" >&2
      echo "  expected $sha256" >&2
      echo "  actual   $actual" >&2
      exit 1
    fi
  fi

  echo "==> Extracting into $dest"
  rm -rf "$dest"
  mkdir -p "$dest"
  tar -xzf "$cache" -C "$dest" --strip-components=1

  # Provenance and licensing, kept next to the binaries so it travels with them.
  cat >"$dest/SOURCE.txt" <<EOF
libmpv XCFrameworks for $platform, vendored by scripts/fetch-ios-mpv.sh.

source:  $url
version: $version ($variant)
sha256:  $sha256
contents: mpv 0.36.0, FFmpeg 6.0, libass, freetype, harfbuzz, fribidi, dav1d,
          mbedtls, libxml2, uchardet, libpng.

These are dynamic frameworks and are embedded in the app bundle, so the LGPL
obligation to allow replacement is met by the dynamic link itself. mpv and
FFmpeg are LGPL-2.1-or-later; the default variant has no GPL components
(-Dgpl=false).
EOF

  echo "==> Vendored $platform frameworks:"
  ls "$dest" | grep '\.xcframework$' | sed 's/^/    /'
  echo
  echo "libmpv $version ($platform) -> $dest"
}

for platform in $platforms; do
  case "$platform" in
  ios)
    fetch iOS "$ios_variant" "$ios_sha256" "apps/ios/native/mpv"
    ;;
  macos)
    fetch macOS "$macos_variant" "$macos_sha256" "apps/macos/native/mpv"
    ;;
  *)
    echo "Unknown platform '$platform' (expected ios or macos)" >&2
    exit 1
    ;;
  esac
done
