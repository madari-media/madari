#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# The TV web settings UI is embedded into the native library with include_str!.
# Build it first so the shipped assets match the OpenAPI client.
if [[ "${MADARI_SKIP_WEB_BUILD:-}" != "1" ]]; then
  web="crates/madari-tv/web"
  if [[ ! -d "$web/node_modules" ]]; then
    echo "Missing $web/node_modules. Run 'npm install' there, or set MADARI_SKIP_WEB_BUILD=1 to reuse the existing dist." >&2
    exit 1
  fi
  (cd "$web" && npm run build)
fi

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
ndk="${ANDROID_NDK_HOME:-$sdk/ndk/28.2.13676358}"
toolchain="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
if [[ ! -x "$toolchain/aarch64-linux-android26-clang" ]]; then
  echo "Set ANDROID_NDK_HOME to an installed Linux Android NDK (28.2 recommended)." >&2
  exit 1
fi
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/aarch64-linux-android26-clang"
export CC_aarch64_linux_android="$toolchain/aarch64-linux-android26-clang"
export AR_aarch64_linux_android="$toolchain/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
# Bundle the Java trust-store adapter that matches Cargo.lock.
verifier_manifest=$(cargo metadata --locked --format-version 1 --filter-platform aarch64-linux-android | python3 -c 'import json,sys; print(next(p["manifest_path"] for p in json.load(sys.stdin)["packages"] if p["name"] == "rustls-platform-verifier-android"))')
mkdir -p apps/tv/app/libs
verifier_dir="$(dirname "$verifier_manifest")"
cp "$verifier_dir/maven/rustls/rustls-platform-verifier/0.1.1/rustls-platform-verifier-0.1.1.aar" apps/tv/app/libs/
cargo build --locked -p madari-tv --target aarch64-linux-android --release
mkdir -p apps/tv/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/libmadari_tv.so apps/tv/app/src/main/jniLibs/arm64-v8a/
"$toolchain/llvm-strip" --strip-unneeded apps/tv/app/src/main/jniLibs/arm64-v8a/libmadari_tv.so

# Many Google TV devices expose a 32-bit Android userspace, even on ARM64 chips.
export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$toolchain/armv7a-linux-androideabi26-clang"
export CC_armv7_linux_androideabi="$toolchain/armv7a-linux-androideabi26-clang"
export AR_armv7_linux_androideabi="$toolchain/llvm-ar"
export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
cargo build --locked -p madari-tv --target armv7-linux-androideabi --release
mkdir -p apps/tv/app/src/main/jniLibs/armeabi-v7a
cp target/armv7-linux-androideabi/release/libmadari_tv.so apps/tv/app/src/main/jniLibs/armeabi-v7a/
"$toolchain/llvm-strip" --strip-unneeded apps/tv/app/src/main/jniLibs/armeabi-v7a/libmadari_tv.so
