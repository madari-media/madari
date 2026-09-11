#!/usr/bin/env bash
# Type-checks the app for macOS without SwiftPM's cross-compilation path.
#
# Why this exists: on Linux, `swift build --swift-sdk darwin --triple arm64-apple-macosx`
# works only about half the time. xtool's Darwin bundle ships both `info.json` (the legacy
# artifact-bundle form) and `swift-sdk.json` (the newer Swift SDK form), SwiftPM registers
# it twice under the same id `darwin`, and then picks between them arbitrarily — when it
# picks the legacy parse it applies an iOS or simulator sysroot and the build dies with
# "unable to load standard library for target 'arm64-apple-macosx14.0'".
#
# Nothing here links or assembles anything: it drives swiftc directly with the SDK and the
# macOS stdlib from the bundle, which is deterministic and enough to catch the porting
# errors. Building the real app is scripts/build-macos.sh, and on a Mac that is the only
# path that matters.
set -euo pipefail
cd "$(dirname "$0")/.."

bundle="${MADARI_DARWIN_BUNDLE:-$HOME/.config/swiftpm/swift-sdks/darwin.artifactbundle}"
if [[ "$(uname -s)" == "Darwin" ]]; then
  sdk="${MADARI_MACOS_SDK:-$(xcrun --sdk macosx --show-sdk-path)}"
  resources="$(dirname "$(xcrun --find swiftc)")/../lib/swift"
  swiftc_bin="$(xcrun --find swiftc)"
else
  sdk="${MADARI_MACOS_SDK:-$bundle/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk}"
  resources="$bundle/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift"
  swiftc_bin="${MADARI_SWIFTC:-/usr/lib/swift/bin/swiftc}"
fi
deployment="${MADARI_MACOS_MIN:-14.0}"
target="arm64-apple-macosx$deployment"

[[ -d "$sdk" ]] || { echo "Missing the macOS SDK at $sdk." >&2; exit 1; }
[[ -d "$resources/macosx" ]] || { echo "No macOS stdlib at $resources/macosx." >&2; exit 1; }

frameworks="$PWD/apps/ios/native/mpv-macos"
[[ -d "$frameworks" ]] || {
  echo "Missing the macOS libmpv frameworks. Run:" >&2
  echo "  MADARI_MPV_PLATFORMS='ios macos' bash scripts/fetch-ios-mpv.sh" >&2
  exit 1
}

# -F for every framework, since the Swift sources import them by module name.
framework_flags=()
for xcframework in "$frameworks"/*.xcframework; do
  slice="$(find "$xcframework" -maxdepth 1 -type d -name 'macos-*' | head -1)"
  [[ -n "$slice" ]] && framework_flags+=("-F" "$slice")
done

common=(
  -target "$target"
  -sdk "$sdk"
  -resource-dir "$resources"
  -Xcc "-DGL_SILENCE_DEPRECATION"
  -Xcc "-DGLES_SILENCE_DEPRECATION"
  -I apps/ios/Sources/madari_iosFFI
)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# SwiftPM generates a `Bundle.module` accessor for a target that has resources. This
# harness does not run SwiftPM, so without this every `Bundle.module` use would look like
# an error here and nowhere else.
cat >"$work/ResourceBundleAccessor.swift" <<'EOF'
import Foundation

extension Bundle {
    static var module: Bundle { .main }
}
EOF

# The bindings are a separate module, so they are compiled to a .swiftmodule first and the
# app is checked against it, exactly as it would be when built.
echo "==> Type-checking MadariCore"
"$swiftc_bin" -emit-module -emit-module-path "$work/MadariCore.swiftmodule" \
  -module-name MadariCore "${common[@]}" \
  $(find apps/ios/Sources/MadariCore -name '*.swift')

echo "==> Type-checking Madari"
mapfile -t sources < <(find apps/ios/Sources/Madari -name '*.swift')
"$swiftc_bin" -typecheck -module-name Madari "${common[@]}" -I "$work" \
  "${framework_flags[@]}" "${sources[@]}" "$work/ResourceBundleAccessor.swift"
