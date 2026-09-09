#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
sysroot="$PWD/target/windows-deps"
if [[ ! -f "$sysroot/.complete" ]]; then
    python3 scripts/fetch-windows-dependencies.py
fi
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR_x86_64_pc_windows_gnu="$sysroot"
export PKG_CONFIG_LIBDIR_x86_64_pc_windows_gnu="$sysroot/mingw64/lib/pkgconfig:$sysroot/mingw64/share/pkgconfig"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-L native=$sysroot/mingw64/lib"
cargo build --locked --release -p madari-linux --target x86_64-pc-windows-gnu
python3 scripts/package-windows.py
