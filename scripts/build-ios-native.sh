#!/usr/bin/env bash
# Builds the Rust core for iOS, then regenerates the committed Swift bindings into
# apps/ios/Sources. This is the iOS counterpart of scripts/build-tv-native.sh.
#
# Set MADARI_IOS_BINDINGS_CHECK=1 to regenerate into a temporary directory instead
# and fail when the committed bindings are stale. CI uses that.
set -euo pipefail
cd "$(dirname "$0")/.."

# The Darwin Swift SDK that xtool installs; this is a Linux cross build with no Xcode.
bundle="${MADARI_DARWIN_BUNDLE:-$HOME/.config/swiftpm/swift-sdks/darwin.artifactbundle}"
sdk="${MADARI_IOS_SDK:-$bundle/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk}"
target="${MADARI_IOS_TARGET:-aarch64-apple-ios}"
clang="${MADARI_IOS_CLANG:-/usr/lib/swift/bin/clang}"
deployment="${MADARI_IOS_MIN:-17.0}"

if [[ ! -d "$sdk" ]]; then
  echo "Missing the iPhoneOS SDK at $sdk. Run 'xtool setup' (see docs/ios.md)." >&2
  exit 1
fi
if [[ ! -x "$clang" ]]; then
  echo "Missing the Swift toolchain clang at $clang." >&2
  exit 1
fi

# The TV web settings UI is embedded into the native library with include_str!.
# Build it first so the shipped assets match the OpenAPI client.
if [[ "${MADARI_SKIP_WEB_BUILD:-}" != "1" ]]; then
  web="crates/madari-tv/web"
  if [[ ! -d "$web/node_modules" ]]; then
    echo "Missing $web/node_modules. Run 'pnpm install' there, or set MADARI_SKIP_WEB_BUILD=1 to reuse the existing dist." >&2
    exit 1
  fi
  (cd "$web" && pnpm run build)
fi

# cc-rs resolves Apple SDK paths through xcrun unless SDKROOT is already set, and
# there is no xcrun on Linux. rusqlite's bundled SQLite and network-interface's C
# helper both go through cc-rs, so this is required for any iOS build here.
# Only set where there is no xcrun to resolve the path. On macOS this variable is not
# target-scoped, so it also reaches the macOS build scripts cargo compiles on the way to
# the iOS target, whose link then fails against iPhoneOS libraries.
if [[ "$(uname -s)" != "Darwin" ]]; then
  export SDKROOT="$sdk"
fi
export IPHONEOS_DEPLOYMENT_TARGET="$deployment"
export CARGO_TARGET_AARCH64_APPLE_IOS_LINKER="$clang"
export CC_aarch64_apple_ios="$clang"
export CXX_aarch64_apple_ios="$clang++"
export AR_aarch64_apple_ios="${MADARI_IOS_AR:-ar}"
export CFLAGS_aarch64_apple_ios="-isysroot $sdk -target arm64-apple-ios$deployment"
export CXXFLAGS_aarch64_apple_ios="-isysroot $sdk -target arm64-apple-ios$deployment"
# Same reason as SDKROOT: the global form applies to the host targets too, so on macOS
# the sysroot is handed to the iOS target alone.
if [[ "$(uname -s)" == "Darwin" ]]; then
  export CARGO_TARGET_AARCH64_APPLE_IOS_RUSTFLAGS="-C link-arg=-isysroot -C link-arg=$sdk"
else
  export RUSTFLAGS="-C link-arg=-isysroot -C link-arg=$sdk"
fi

# Only the static library is linked into the app; building the cdylib too would
# produce a second, unused artifact for every build. Skipped by the CI bindings
# check, which needs no Darwin SDK because bindgen only reads a host build.
if [[ "${MADARI_IOS_SKIP_TARGET:-}" != "1" ]]; then
  cargo rustc --locked -p madari-ios --target "$target" --release --crate-type staticlib
  mkdir -p apps/ios/native
  cp "target/$target/release/libmadari_ios.a" apps/ios/native/
fi

# `--library` reads the metadata out of a host build of the same crate, so the
# bindings cannot drift from the Rust definitions they describe. The crate
# declares no cdylib (that would break the iOS link), so ask for one here.
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
cargo rustc --locked -q -p madari-ios --crate-type cdylib
# A cdylib is only named .so where ELF is the object format. On macOS it is a .dylib,
# and naming the Linux path there failed with a bare "No such file or directory" after
# the build itself had already succeeded.
extension="so"
if [[ "$(uname -s)" == "Darwin" ]]; then
  extension="dylib"
fi
cargo run --locked -q -p madari-ios --features cli --bin uniffi-bindgen -- \
  generate --library "target/debug/libmadari_ios.$extension" --language swift --out-dir "$out"

install_bindings() {
  local source="$1"
  mkdir -p "$source/madari_iosFFI" "$source/MadariCore"
  mv "$out/madari_iosFFI.h" "$source/madari_iosFFI/"
  # SwiftPM looks for `module.modulemap` in a C target's public headers path.
  mv "$out/madari_iosFFI.modulemap" "$source/madari_iosFFI/module.modulemap"
  mv "$out/MadariCore.swift" "$source/MadariCore/"
}

if [[ "${MADARI_IOS_BINDINGS_CHECK:-}" == "1" ]]; then
  generated="$(mktemp -d)"
  install_bindings "$generated"
  if ! diff -r "$generated/MadariCore" apps/ios/Sources/MadariCore >/dev/null 2>&1 \
     || ! diff -r "$generated/madari_iosFFI" apps/ios/Sources/madari_iosFFI >/dev/null 2>&1; then
    echo "::error::apps/ios/Sources bindings are stale. Run scripts/build-ios-native.sh." >&2
    diff -ru apps/ios/Sources/MadariCore "$generated/MadariCore" || true
    diff -ru apps/ios/Sources/madari_iosFFI "$generated/madari_iosFFI" || true
    exit 1
  fi
  echo "iOS bindings are up to date."
else
  install_bindings apps/ios/Sources
  echo "Wrote apps/ios/native/libmadari_ios.a and refreshed apps/ios/Sources bindings."
fi
