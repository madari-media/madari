#!/usr/bin/env bash
# Builds the native macOS app: the Rust core for the darwin targets, then the Swift app,
# then assembles Madari.app around it.
#
# This is the macOS counterpart of what xtool does for iOS. xtool only targets iOS (its
# subcommands are devices/install/dev and it builds for arm64-apple-ios), so the bundle is
# assembled here: Contents/MacOS/Madari, Contents/Frameworks with the 18 vendored libmpv
# frameworks, Contents/Resources with the SwiftPM resource bundle.
#
# Set MADARI_SKIP_WEB_BUILD=1 to reuse the existing web/dist, and MADARI_MACOS_ARCHS to
# build fewer architectures (the default is a universal arm64 + x86_64 app).
set -euo pipefail
cd "$(dirname "$0")/.."

deployment="${MADARI_MACOS_MIN:-14.0}"
archs="${MADARI_MACOS_ARCHS:-arm64 x86_64}"
configuration="${MADARI_MACOS_CONFIG:-release}"
output="apps/macos/build/Madari.app"

# The SDK and clang come from Xcode on a Mac and from the Darwin Swift SDK elsewhere, the
# same split scripts/build-ios-native.sh uses.
if [[ "$(uname -s)" == "Darwin" ]]; then
  sdk="${MADARI_MACOS_SDK:-$(xcrun --sdk macosx --show-sdk-path)}"
  clang="${MADARI_MACOS_CLANG:-$(xcrun --find clang)}"
else
  bundle="${MADARI_DARWIN_BUNDLE:-$HOME/.config/swiftpm/swift-sdks/darwin.artifactbundle}"
  sdk="${MADARI_MACOS_SDK:-$bundle/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk}"
  clang="${MADARI_MACOS_CLANG:-/usr/lib/swift/bin/clang}"
fi

if [[ ! -d "$sdk" ]]; then
  echo "Missing the macOS SDK at $sdk." >&2
  echo "On a Mac this comes from Xcode; elsewhere run 'xtool setup' (see docs/ios.md)." >&2
  exit 1
fi
if [[ ! -x "$clang" ]]; then
  echo "Missing clang at $clang." >&2
  exit 1
fi

# Both are published by the same release as the iOS frameworks, but the macOS app is a
# separate build, so this checks rather than assuming.
ios_libraries="$PWD/apps/ios/native/mpv"
macos_libraries="$PWD/apps/ios/native/mpv-macos"
if [[ ! -d "$macos_libraries" ]]; then
  echo "Missing the macOS libmpv frameworks at $macos_libraries." >&2
  echo "Run: MADARI_MPV_PLATFORMS='ios macos' bash scripts/fetch-ios-mpv.sh" >&2
  exit 1
fi

# The committed UniFFI bindings are shared with iOS, so they are not regenerated here.
if [[ ! -d "apps/ios/Sources/MadariCore" ]]; then
  echo "Missing the committed Swift bindings. Run scripts/build-ios-native.sh first." >&2
  exit 1
fi

# The TV web settings UI is embedded into the native library with include_str!.
if [[ "${MADARI_SKIP_WEB_BUILD:-}" != "1" ]]; then
  web="crates/madari-tv/web"
  if [[ ! -d "$web/node_modules" ]]; then
    echo "Missing $web/node_modules. Run 'pnpm install' there, or set MADARI_SKIP_WEB_BUILD=1 to reuse the existing dist." >&2
    exit 1
  fi
  (cd "$web" && pnpm run build)
fi

# A universal binary needs a lipo. llvm-lipo is what the Linux Swift toolchain ships;
# macOS has the real one. Without either, the first architecture is used and the rest are
# reported rather than silently dropped.
lipo="$(command -v lipo || command -v llvm-lipo || true)"

rust_target_for() {
  case "$1" in
  arm64) echo "aarch64-apple-darwin" ;;
  x86_64) echo "x86_64-apple-darwin" ;;
  *)
    echo "Unknown architecture '$1'" >&2
    exit 1
    ;;
  esac
}

clang_arch_for() {
  case "$1" in
  arm64) echo "arm64" ;;
  x86_64) echo "x86_64" ;;
  esac
}

# MARK: - Rust core

rust_libraries=()
for arch in $archs; do
  target="$(rust_target_for "$arch")"
  if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
    echo "Missing the Rust target $target. Run: rustup target add $target" >&2
    exit 1
  fi
  echo "==> Rust core for $target"
  # Scoped per target, not exported globally: a global SDKROOT also reaches the host build
  # scripts cargo compiles on the way, and an iOS or macOS sysroot applied to a macOS host
  # object fails to link. This is the same trap scripts/build-ios-native.sh documents.
  target_env="$(echo "${target//-/_}" | tr '[:lower:]' '[:upper:]')"
  env \
    "CARGO_TARGET_${target_env}_LINKER=$clang" \
    "CARGO_TARGET_${target_env}_RUSTFLAGS=-C link-arg=-isysroot -C link-arg=$sdk" \
    "CC_${target//-/_}=$clang" \
    "CXX_${target//-/_}=$clang++" \
    "CFLAGS_${target//-/_}=-isysroot $sdk -target $(clang_arch_for "$arch")-apple-macos$deployment" \
    "CXXFLAGS_${target//-/_}=-isysroot $sdk -target $(clang_arch_for "$arch")-apple-macos$deployment" \
    cargo rustc --locked -p madari-ios --target "$target" --release --crate-type staticlib
  rust_libraries+=("target/$target/release/libmadari_ios.a")
done

mkdir -p apps/ios/native/macos
if [[ ${#rust_libraries[@]} -gt 1 && -n "$lipo" ]]; then
  "$lipo" -create -output apps/ios/native/macos/libmadari_ios.a "${rust_libraries[@]}"
elif [[ ${#rust_libraries[@]} -gt 1 ]]; then
  echo "::warning::no lipo found, so only ${rust_libraries[0]} is used" >&2
  cp "${rust_libraries[0]}" apps/ios/native/macos/libmadari_ios.a
else
  cp "${rust_libraries[0]}" apps/ios/native/macos/libmadari_ios.a
fi
echo "==> Rust core -> apps/ios/native/macos/libmadari_ios.a"

# MARK: - Swift app

swift_build_path="apps/macos/.build"

# SwiftPM registers xtool's Darwin bundle twice — once from its `info.json` (the legacy
# artifact-bundle form) and once from `swift-sdk.json` — and then picks between the two
# arbitrarily. When it picks the legacy parse it applies an iOS or simulator sysroot and
# fails with "unable to load standard library for target 'arm64-apple-macosx14.0'" before
# compiling anything. That is an SDK-selection bug, not a build failure, so it is retried;
# any other error is reported immediately and unchanged. Building on a Mac does not use a
# Swift SDK at all and is not affected.
swift_build() {
  local log="$swift_build_path/swift-build.log"
  mkdir -p "$swift_build_path"
  local attempt
  for attempt in 1 2 3; do
    if (cd apps/macos && MADARI_TARGET_PLATFORM=macos swift build "$@") >"$log" 2>&1; then
      return 0
    fi
    if ! grep -q "unable to load standard library for target" "$log"; then
      grep -E "error:" "$log" | sort -u | head -30 >&2 || true
      tail -5 "$log" >&2
      return 1
    fi
    echo "==> SwiftPM selected the wrong Swift SDK (attempt $attempt of 3); retrying"
  done
  echo "==> SwiftPM kept selecting the wrong Swift SDK. Build the macOS app on a Mac instead." >&2
  return 1
}

# One build per architecture, combined with lipo. SwiftPM's own `--arch arm64 --arch x86_64`
# is deliberately not used: on macOS it fails on the C target with "Build input file cannot
# be found: madari_iosFFI_Module.o", because that multi-architecture product layout does not
# carry the per-architecture module object. The Linux path has to build per triple anyway,
# so both hosts now take the same route.
for arch in $archs; do
  echo "==> Swift app for $arch"
  if [[ "$(uname -s)" == "Darwin" ]]; then
    swift_build -c "$configuration" --triple "${arch}-apple-macosx${deployment}"
  else
    swift_build -c "$configuration" --swift-sdk darwin --triple "${arch}-apple-macosx${deployment}"
  fi
done

executables=()
for arch in $archs; do
  executables+=("$swift_build_path/${arch}-apple-macosx/$configuration/Madari-App")
done
built_executable="${executables[0]}"
resource_bundle="$(find "$swift_build_path" -maxdepth 4 -name 'Madari_Madari.bundle' | head -1)"
if [[ ${#executables[@]} -gt 1 ]]; then
  if [[ -n "$lipo" ]]; then
    "$lipo" -create -output "$swift_build_path/Madari-universal" "${executables[@]}"
    built_executable="$swift_build_path/Madari-universal"
  else
    echo "::warning::no lipo found, so the app is ${archs%% *} only" >&2
  fi
fi

if [[ -z "$built_executable" || ! -f "$built_executable" ]]; then
  echo "The Swift build produced no executable at $built_executable." >&2
  find "$swift_build_path" -name 'Madari-App' -type f 2>/dev/null | head -5 >&2 || true
  exit 1
fi

# MARK: - Bundle

# The version comes from version.txt, so a release cannot ship a bundle whose version
# disagrees with its tag. Read once, used by both plists below.
version="$(cat version.txt 2>/dev/null || echo "0.0.0")"

echo "==> Assembling $output"
rm -rf "$output"
mkdir -p "$output/Contents/MacOS" "$output/Contents/Resources" "$output/Contents/Frameworks"

install -m 0755 "$built_executable" "$output/Contents/MacOS/Madari"
if [[ -n "$resource_bundle" ]]; then
  cp -R "$resource_bundle" "$output/Contents/Resources/"
  # SwiftPM's resource bundle holds the files but ships no Info.plist, and macOS's
  # `Bundle(url:)` refuses a directory without one. That is what killed the first macOS
  # build at launch: `Bundle.module` could not find the bundle, and it reports that by
  # calling fatalError from inside its own initialiser, on the main thread, in App.init().
  # A bundle needs the manifest to be a bundle.
  cat >"$output/Contents/Resources/Madari_Madari.bundle/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleIdentifier</key>
	<string>media.madari.macos.resources</string>
	<key>CFBundleName</key>
	<string>Madari_Madari</string>
	<key>CFBundlePackageType</key>
	<string>BNDL</string>
	<key>CFBundleShortVersionString</key>
	<string>$version</string>
</dict>
</plist>
PLIST
  # The files also go in loose, which is the first place AppResources looks, so loading a
  # font does not depend on bundle parsing at all.
  cp -R "$resource_bundle"/. "$output/Contents/Resources/"
else
  echo "::error::the SwiftPM resource bundle was not found; fonts and the logo live in it" >&2
  exit 1
fi

# The frameworks carry @rpath install names, which resolve against the
# @executable_path/../Frameworks rpath the builder target sets.
for xcframework in "$macos_libraries"/*.xcframework; do
  slice="$(find "$xcframework" -maxdepth 1 -type d -name 'macos-*' | head -1)"
  framework="$(find "$slice" -maxdepth 1 -type d -name '*.framework' | head -1)"
  if [[ -z "$framework" ]]; then
    echo "::error::no macOS slice in $(basename "$xcframework")" >&2
    exit 1
  fi
  cp -R "$framework" "$output/Contents/Frameworks/"
done

printf 'APPL????' >"$output/Contents/PkgInfo"

sed "s/__VERSION__/$version/g" apps/macos/Info.plist >"$output/Contents/Info.plist"

# The resource bundle must be a bundle, or the app cannot find its own font. Checked here
# because this failure is invisible until launch.
test -f "$output/Contents/Resources/Madari_Madari.bundle/Info.plist" \
  || { echo "::error::the resource bundle has no Info.plist" >&2; exit 1; }
test -f "$output/Contents/Resources/DMSans.ttf" \
  || { echo "::error::the font was not copied loose into Resources" >&2; exit 1; }

# `Bundle.module` calls fatalError from inside its own initialiser when it cannot find its
# bundle, which on macOS it cannot, so the app must not use it: AppResources resolves the
# resources directly instead. This is a launch-time crash if it comes back, so it is a build
# failure here instead.
if grep -rn 'Bundle\.module' apps/ios/Sources >/dev/null 2>&1; then
  echo "::error::Bundle.module is used again; on macOS it crashes the app at launch" >&2
  grep -rn 'Bundle\.module' apps/ios/Sources >&2
  exit 1
fi

# MARK: - Sign

# Ad-hoc: enough for the app to launch locally, and the frameworks have to be signed before
# the bundle that contains them. A distributed build is signed and notarised with a real
# identity instead.
if command -v codesign >/dev/null 2>&1; then
  for framework in "$output/Contents/Frameworks/"*.framework; do
    codesign --force --sign - --timestamp=none "$framework" >/dev/null 2>&1 || true
  done
  codesign --force --sign - --timestamp=none "$output" 2>&1 | tail -1 || true
fi

# MARK: - Report

frameworks="$(find "$output/Contents/Frameworks" -maxdepth 1 -name '*.framework' | wc -l | tr -d ' ')"
echo
echo "==> $output"
echo "    version    $version"
echo "    frameworks $frameworks"
echo "    executable $(du -h "$output/Contents/MacOS/Madari" | cut -f1)"
if command -v llvm-objdump >/dev/null 2>&1; then
  echo "    arches     $(llvm-objdump --macho --archs "$output/Contents/MacOS/Madari" 2>/dev/null | tr '\n' ' ' || echo unknown)"
fi

if [[ "$frameworks" != "18" ]]; then
  echo "::error::expected 18 embedded frameworks, found $frameworks" >&2
  exit 1
fi
