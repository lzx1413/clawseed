#!/usr/bin/env python3
"""Build the pinned static tawcroot launcher for Android native libraries.

The launcher is intentionally kept separate from the large Linux rootfs. The
source archive is SHA-256 pinned and the output is audited as a static ET_EXEC
with no interpreter or dynamic dependencies.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = {
    "repo": "wmww/tawc",
    "commit": "a49a4075228f4801dadc1eee584aed33fe8ea4ce",
    "url": "https://github.com/wmww/tawc/archive/a49a4075228f4801dadc1eee584aed33fe8ea4ce.zip",
    "sha256": "74487243bde86c303e786e79cd4a13e98c3279cae4ab7725dcf5fa51c0b530c1",
    "root": "tawc-a49a4075228f4801dadc1eee584aed33fe8ea4ce",
    "ndk": "28.2.13676358",
    "api": 29,
}
ABIS = {"arm64-v8a": ("aarch64", 183, "0x2000000000"), "x86_64": ("x86_64", 62, "0x40000000")}
SOURCES = [
    "src/main.c", "src/io.c", "src/strings.c", "src/filter.c", "src/filter_build.c", "src/handler.c",
    "src/dispatch.c", "src/rescue.c", "src/path.c", "src/path_scratch.c", "src/path_fold.c",
    "src/path_orchestrate.c", "src/path_resolve.c", "src/proc_rewrite.c", "src/proc_shadow.c",
    "src/identity.c", "src/syscalls_fs.c", "src/syscalls_fs_legacy.c", "src/syscalls_link.c",
    "src/syscalls_stat.c", "src/syscalls_xattr.c", "src/syscalls_fd.c", "src/linkstore.c",
    "src/dirent_filter.c", "src/syscalls_control.c", "src/chroot.c", "src/shm.c", "src/signal_shadow.c",
    "src/sigalt.c", "src/syscalls_socket.c", "src/rtnl_emu.c", "src/usercopy.c", "src/loader_elf.c",
    "src/loader_map.c", "src/loader_stack.c", "src/loader_io_prod.c", "src/loader_exec.c",
    "src/proctitle.c", "src/exec_state.c", "src/exec_handler.c", "src/syscalls_exec.c", "src/supervisor.c",
]


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def archive(cache, supplied, offline):
    cache.mkdir(parents=True, exist_ok=True)
    target = cache / f"{SOURCE['commit']}.zip"
    source = Path(supplied) if supplied else target
    if not source.is_file():
        if offline:
            raise SystemExit(f"missing offline tawcroot archive: {source}")
        pending = target.with_suffix(".zip.part")
        with urllib.request.urlopen(SOURCE["url"], timeout=60) as incoming, pending.open("wb") as out:
            shutil.copyfileobj(incoming, out)
        os.replace(pending, target)
        source = target
    if digest(source) != SOURCE["sha256"]:
        raise SystemExit(f"tawcroot archive SHA-256 mismatch: {source}")
    return source


def extract(archive_path, destination):
    prefix = SOURCE["root"] + "/tawcroot/"
    with zipfile.ZipFile(archive_path) as archive_file:
        for member in archive_file.infolist():
            if member.is_dir() or not member.filename.startswith(prefix):
                continue
            relative = member.filename[len(prefix):]
            if not (relative.startswith("src/") or relative.startswith("include/")):
                continue
            output = destination / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            with archive_file.open(member) as incoming, output.open("wb") as out:
                shutil.copyfileobj(incoming, out)


def ndk_tools(ndk):
    host = {"linux": "linux-x86_64", "darwin": "darwin-x86_64", "win32": "windows-x86_64"}[sys_platform()]
    binary = Path(ndk) / "toolchains/llvm/prebuilt" / host / "bin"
    properties = {
        key.strip(): value.strip()
        for line in (Path(ndk) / "source.properties").read_text().splitlines()
        if "=" in line
        for key, value in [line.split("=", 1)]
    }
    if properties.get("Pkg.Revision") != SOURCE["ndk"]:
        raise SystemExit(f"tawcroot requires NDK {SOURCE['ndk']}")
    return binary / "clang", binary / "llvm-strip"


def sys_platform():
    name = platform.system().lower()
    return "win32" if name.startswith("windows") else name


def build(ndk, archive_path, output):
    clang, llvm_strip = ndk_tools(ndk)
    output = Path(output)
    work = Path(tempfile.mkdtemp(prefix="tawcroot-", dir=output.parent))
    try:
        source = work / "source"
        extract(archive_path, source)
        staged = work / "out"
        for abi, (arch, machine, image_base) in ABIS.items():
            objects = work / "objects" / abi
            objects.mkdir(parents=True)
            flags = [f"--target={arch}-linux-android{SOURCE['api']}", "-O2", "-fno-strict-aliasing",
                     "-fno-stack-protector", "-fvisibility=hidden", "-fno-exceptions",
                     "-fno-asynchronous-unwind-tables", "-ffreestanding", "-Wall", "-Wextra", "-Werror",
                     "-Wframe-larger-than=1024", "-Iinclude"]
            if arch == "aarch64":
                flags.append("-mno-outline-atomics")
            object_paths = []
            names = SOURCES + [f"src/arch/{arch}_stub.S", f"src/arch/{arch}_loader_jump.S"]
            for index, name in enumerate(names):
                obj = objects / f"{index}.o"
                subprocess.run([str(clang), *flags, "-c", name, "-o", str(obj)], cwd=source, check=True)
                object_paths.append(str(obj))
            artifact = staged / "jniLibs" / abi / "libtawcroot.so"
            artifact.parent.mkdir(parents=True)
            link = [f"--target={arch}-linux-android{SOURCE['api']}", "-fuse-ld=lld", "-static", "-nostdlib",
                    "-nostartfiles", "-Wl,--build-id=none", "-Wl,-z,noexecstack", "-Wl,-z,relro",
                    "-Wl,-z,max-page-size=16384", f"-Wl,--image-base={image_base}", "-Wl,-soname,libtawcroot.so"]
            subprocess.run([str(clang), *link, *object_paths, "-o", str(artifact)], cwd=source, check=True)
            subprocess.run([str(llvm_strip), str(artifact)], check=True)
            data = artifact.read_bytes()
            if data[:4] != b"\x7fELF" or data[4] != 2 or data[18:20] != machine.to_bytes(2, "little"):
                raise SystemExit(f"{abi}: tawcroot artifact is not a 64-bit ELF for the expected machine")
            if b"PT_INTERP" in data or b"DT_NEEDED" in data:
                raise SystemExit(f"{abi}: tawcroot artifact must be static")
        manifest = {"schema": 1, "source": SOURCE, "artifacts": {
            abi: {"sha256": digest(staged / "jniLibs" / abi / "libtawcroot.so"),
                  "bytes": (staged / "jniLibs" / abi / "libtawcroot.so").stat().st_size}
            for abi in ABIS}}
        (staged / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        if output.exists():
            shutil.rmtree(output)
        os.replace(staged, output)
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--ndk", required=True)
    parser.add_argument("--archive")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--output", default="build/tawcroot")
    args = parser.parse_args()
    build(args.ndk, archive(Path(args.output).parent / ".cache/tawcroot", args.archive, args.offline), args.output)
