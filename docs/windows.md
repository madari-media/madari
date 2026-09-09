# Experimental Windows desktop build

The Windows x64 build reuses the GTK4/libadwaita desktop client and embedded mpv
player. The Cargo package is still named `madari-linux`; the packaged executable
is `Madari.exe`. This is an experimental cross-compiled build for VM testing.

## Run in a VM

1. Copy `target/windows/Madari-windows-x64.zip` to a Windows 10/11 x64 VM.
2. Extract the entire ZIP to a writable folder.
3. Open `Madari/Start Madari.cmd`. Keep all bundled files together.

The launcher selects GTK's OpenGL renderer and captures output in `madari.log`.
Install the VM's guest graphics tools and enable 3D acceleration for embedded
video playback. Actual graphics and audio behavior must be checked in the VM.

Profiles, settings, artwork and bundled fonts are stored under
`%LOCALAPPDATA%\Madari`. Set `MADARI_DATA_DIR` to override that location.
Direct HTTP(S) playback is included. Torrent playback still needs a separately
running companion server configured in Settings; the ZIP does not contain it.

For a first test, create a profile, install an addon, browse a catalog, play a
direct source, seek, and close/reopen the app to check persisted state. Then
check fullscreen, audio/subtitle selection and PiP. Send `madari.log` if startup
or playback fails.

## Rebuild on Linux

Prerequisites: Rust 1.96+, the `x86_64-pc-windows-gnu` Rust target, a MinGW-w64
x64 GCC/binutils toolchain, host C/C++ tools and CMake, `pkg-config`, Python 3,
curl, tar with zstd support, and the host `glib-compile-schemas` command.

```sh
rustup target add x86_64-pc-windows-gnu
bash scripts/build-windows.sh
```

The dependency fetcher downloads the MSYS2 packages pinned in
`scripts/windows-dependencies.json`, verifies SHA-256 checksums and extracts
them under `target/windows-deps`. It does not install system packages.
The build script sets target-specific compiler and pkg-config paths, builds
the desktop release, then packages its transitive DLL imports, dynamically
loaded libepoxy, GDK image loaders, schemas, icons and license files.

Outputs:

- `target/windows/Madari-windows-x64.zip`: transferable test package.
- `target/windows/Madari/`: unpacked equivalent.
- `target/x86_64-pc-windows-gnu/release/madari-linux.exe`: unbundled executable.

`dependencies.json` records the bundled native package versions and checksums;
`system-dlls.json` lists imports supplied by Windows. Native package build
recipes are maintained at <https://github.com/msys2/MINGW-packages>.
