#!/usr/bin/env python3
"""Bundle the Windows desktop executable and its native runtime dependencies."""
import json
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
PREFIX = ROOT / "target/windows-deps/mingw64"
OUTPUT = ROOT / "target/windows/Madari"


def main():
    if OUTPUT.exists():
        shutil.rmtree(OUTPUT)
    OUTPUT.mkdir(parents=True)
    shutil.copy2(ROOT / "target/x86_64-pc-windows-gnu/release/madari-linux.exe", OUTPUT / "Madari.exe")
    dlls = {p.name.lower(): p for p in (PREFIX / "bin").glob("*.dll")}
    pending = [OUTPUT / "Madari.exe", dlls["libepoxy-0.dll"]]
    for directory in ["share/glib-2.0/schemas", "share/icons/Adwaita", "share/icons/hicolor",
                      "share/mime", "share/fontconfig", "etc/fonts", "share/licenses",
                      "lib/gdk-pixbuf-2.0", "lib/gio/modules"]:
        source = PREFIX / directory
        if source.exists():
            shutil.copytree(source, OUTPUT / directory)
    pending.extend((OUTPUT / "lib").rglob("*.dll"))
    visited = set()
    system = set()
    while pending:
        path = pending.pop()
        key = path.name.lower()
        if key in visited:
            continue
        visited.add(key)
        if path.parent == PREFIX / "bin":
            shutil.copy2(path, OUTPUT / path.name)
        info = subprocess.check_output(["x86_64-w64-mingw32-objdump", "-p", str(path)], text=True)
        for name in re.findall(r"DLL Name:\s*(\S+)", info):
            dependency = dlls.get(name.lower())
            if dependency:
                pending.append(dependency)
            else:
                system.add(name.lower())
    # Compile relocatable GSettings schemas without needing a Windows executable.
    subprocess.run(["glib-compile-schemas", str(OUTPUT / "share/glib-2.0/schemas")], check=True)
    shutil.copy2(ROOT / "scripts/windows-dependencies.json", OUTPUT / "dependencies.json")
    (OUTPUT / "system-dlls.json").write_text(json.dumps(sorted(system), indent=2) + "\n")
    (OUTPUT / "Start Madari.cmd").write_bytes(
        ('@echo off\nsetlocal\ncd /d "%~dp0"\n'
         'set "PATH=%~dp0;%PATH%"\n'
         'set "XDG_DATA_DIRS=%~dp0share"\n'
         'set "GSETTINGS_SCHEMA_DIR=%~dp0share\\glib-2.0\\schemas"\n'
         'set "GDK_BACKEND=win32"\n'
         'set "GSK_RENDERER=gl"\n'
         '"%~dp0Madari.exe" > "%~dp0madari.log" 2>&1\n').replace('\n', '\r\n').encode()
    )
    (OUTPUT / "README.txt").write_text(
        "Madari - experimental Windows x64 desktop build\n\n"
        "Extract the entire ZIP, then double-click Start Madari.cmd.\n"
        "Keep the DLLs and share/lib/etc directories alongside Madari.exe.\n"
        "Use Windows 10/11 x64 with your VM's graphics tools/3D acceleration enabled.\n"
        "Profiles are stored in %LOCALAPPDATA%\\Madari. MADARI_DATA_DIR overrides this.\n"
        "If startup or playback fails, send madari.log from this folder.\n"
        "This package includes the desktop player, not the torrent companion server.\n"
        "HTTP(S) playback is built in; torrents require a configured companion.\n\n"
        "Third-party versions/checksums: dependencies.json. Licenses: share/licenses.\n"
        "Native dependency sources/build recipes: https://github.com/msys2/MINGW-packages\n"
        "This is a cross-compiled test build; Windows VM playback needs validation.\n"
    )
    archive = shutil.make_archive(str(OUTPUT.parent / "Madari-windows-x64"), "zip", OUTPUT.parent, OUTPUT.name)
    print(f"Packaged {archive} ({Path(archive).stat().st_size / 1024**2:.1f} MiB)")
    print("External system DLLs:", ", ".join(sorted(system)))


if __name__ == "__main__":
    main()
