"""Offline 0031 diagnostic native-XIP ROM builder. Never contacts a device.

Requires a precreated output directory, read-only upstream inputs and its GCC.
All compiler inputs/outputs are explicit. No mkdir, pip, cloud or flashing calls.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import zlib

sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[2]
BASE_VA = 0xA02465D0
PACKAGE_VA = 0xA024C000
PAYLOAD_OFFSET = 0x2475D0
PAYLOAD_CAPACITY = 60934
GIF_OFFSET = 0x1F80AC
GIF_OLD_BYTES = 385834
GIF_NEW_BYTES = 324886
MAP_BASE = 0x9FFFF000
HOOK_VA = 0xA00B2732
HOOK_OFFSET = HOOK_VA - MAP_BASE
TRAMP_VA = 0xA001B0AC
TRAMP_OFFSET = TRAMP_VA - MAP_BASE
BASE_SHA = "8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def wsl_path(path):
    path = Path(path).resolve()
    if not path.drive or len(path.drive) != 2:
        raise ValueError("Windows drive path required")
    return "/mnt/" + path.drive[0].lower() + path.as_posix()[2:]


def run_wsl(*args):
    result = subprocess.run(["wsl", "-d", "Ubuntu-24.04", "--exec", *map(str, args)],
                            capture_output=True, encoding="utf-8", errors="replace", timeout=120)
    if result.returncode:
        raise RuntimeError(f"tool failed ({result.returncode}): {args[0]}\n{result.stderr}\n{result.stdout}")
    return result.stdout


def put(path, content):
    with path.open("x", encoding="utf-8", newline="\n") as handle:
        handle.write(content)


def put_binary(path, content):
    with path.open("xb") as handle:
        handle.write(content)


def jal(source, target):
    offset = target - source
    if offset % 2 or not -(1 << 20) <= offset < (1 << 20):
        raise ValueError("JAL target out of range")
    value = offset & 0x1FFFFF
    return struct.pack("<I", ((value >> 20) << 31) | (((value >> 1) & 1023) << 21)
                       | (((value >> 11) & 1) << 20) | (((value >> 12) & 255) << 12)
                       | (1 << 7) | 0x6F)


def trampoline(target):
    high = (target + 0x800) >> 12
    low = target & 0xFFF
    return struct.pack("<II", (high << 12) | (5 << 7) | 0x37,
                       (low << 20) | (5 << 15) | 0x67)


def build(output, upstream):
    if not output.is_dir() or not output.resolve().is_relative_to(REPO / "artifacts/ap01-research"):
        raise ValueError("Output must be a precreated repository artifacts/ap01-research directory")
    output = output.resolve()
    # A fixed namespace makes reruns refuse rather than silently replacing evidence.
    names = ["app.elf", "app.bin", "app.apx", "base.elf", "payload.bin", "manifest.json",
             "symbols.json", "ap01-1.0.2_0031-app-xip.bin", "original.gif", "optimized.gif",
             "assets.S", "gif-check.json", "app.map", "base.map", "app.dis", "base.dis",
             "app.sections", "base.sections"]
    stems = ["diagnostic_app", "app_loader", "base", "entry", "assets"]
    names += [stem + suffix for stem in stems for suffix in (".o", ".s", ".su")]
    for name in names:
        if (output / name).exists():
            raise FileExistsError(output / name)
    sys.path.insert(0, str(upstream))
    from core.firmware_image import (validate_baseline, validate_candidate,
                                    refresh_recovery_crc, ByteRange, FirmwareValidationError)
    original = (upstream / "artifacts/firmware/original/ap01-1.0.2_0031.bin").read_bytes()
    baseline = validate_baseline(original)
    opt_rom = (upstream / "artifacts/firmware/第三方固件/AGENTS看板/ap01-1.0.2_0031-opt.bin").read_bytes()
    gif_old = original[GIF_OFFSET:GIF_OFFSET + GIF_OLD_BYTES]
    gif_new = opt_rom[GIF_OFFSET:GIF_OFFSET + GIF_NEW_BYTES]
    if sha(gif_old) != "5e656788665f7f0022b0b5500d569a8aae07f4a46b080d033b3cacfa4abc940d" or sha(gif_new) != "d574246a17af026384688799eff1a289ce670cd1c396d7ab799a100d8a3c806b":
        raise ValueError("GIF input identity mismatch")
    if original[HOOK_OFFSET:HOOK_OFFSET + 6] != bytes.fromhex("5285eff02079") or original[TRAMP_OFFSET:TRAMP_OFFSET + 8] != bytes(8):
        raise ValueError("Stock hook/trampoline bytes mismatch")
    if ((GIF_OFFSET + GIF_NEW_BYTES + 15) & ~15) != PAYLOAD_OFFSET or GIF_OFFSET + GIF_OLD_BYTES - PAYLOAD_OFFSET != PAYLOAD_CAPACITY:
        raise ValueError("Payload space mismatch")
    put_binary(output / "original.gif", gif_old)
    put_binary(output / "optimized.gif", gif_new)
    gif_check = json.loads(run_wsl("python3", wsl_path(REPO / "tools/ap01/verify_gif.py"),
                                    wsl_path(output / "original.gif"), wsl_path(output / "optimized.gif")))
    put(output / "gif-check.json", json.dumps(gif_check, indent=2) + "\n")
    tool_dir = upstream / "tools/docker/toolchain/bin"
    prefix = wsl_path(tool_dir) + "/riscv64-unknown-elf-"
    versions = {name: run_wsl(prefix + name, "--version").splitlines()[0]
                for name in ("gcc", "as", "ld", "objcopy")}
    flags = ["-march=rv32imac", "-mabi=ilp32", "-Os", "-ffreestanding", "-fno-builtin",
             "-fno-pic", "-msmall-data-limit=0", "-mno-relax", "-fstack-usage",
             "-Wall", "-Wextra", "-Werror", "-std=c99", "-I" + wsl_path(REPO / "firmware/ap01")]

    def assemble(stem, source, c=False):
        if c:
            run_wsl(prefix + "gcc", "-B" + wsl_path(tool_dir) + "/", *flags,
                    "-S", wsl_path(source), "-o", wsl_path(output / (stem + ".s")))
            source = output / (stem + ".s")
        run_wsl(prefix + "as", "-march=rv32imac", "-mabi=ilp32", "-mno-relax",
                wsl_path(source), "-o", wsl_path(output / (stem + ".o")))

    def link(stem, objects):
        run_wsl(prefix + "ld", "-m", "elf32lriscv", "--no-relax", "-T",
                wsl_path(REPO / ("firmware/ap01/" + stem + ".ld")),
                "-Map=" + wsl_path(output / (stem + ".map")),
                *[wsl_path(output / (name + ".o")) for name in objects],
                "-o", wsl_path(output / (stem + ".elf")))
        elf = output / (stem + ".elf")
        details = run_wsl(prefix + "readelf", "-h", "-S", "-r", wsl_path(elf))
        if "ELF32" not in details or "RISC-V" not in details or "There are no relocations" not in details:
            raise ValueError("ELF class/architecture/relocation check failed")
        put(output / (stem + ".sections"), details)
        put(output / (stem + ".dis"), run_wsl(prefix + "objdump", "-d", wsl_path(elf)))
        symbols = {}
        for line in run_wsl(prefix + "nm", "-n", wsl_path(elf)).splitlines():
            fields = line.split()
            if len(fields) == 3:
                symbols[fields[2]] = int(fields[0], 16)
        return symbols

    assemble("diagnostic_app", REPO / "firmware/ap01/diagnostic_app.c", True)
    app_symbols = link("app", ["diagnostic_app"])
    run_wsl(prefix + "objcopy", "-O", "binary", wsl_path(output / "app.elf"), wsl_path(output / "app.bin"))
    text = (output / "app.bin").read_bytes()
    data_offset = (112 + len(text) + 3) & ~3
    body = text + bytes(data_offset - 112 - len(text)) + struct.pack("<II", 0x31, 7)
    entries = [app_symbols[name] - (PACKAGE_VA + 112)
               for name in ("ap01_diag_init", "ap01_diag_step", "ap01_diag_stop")]
    if any(offset < 0 or offset >= len(text) or offset & 1 for offset in entries):
        raise ValueError("App entry outside text")
    header = bytearray(112)
    words = [0x31585041, 1, 112, 112 + len(body), 1, 1, PACKAGE_VA, 112, len(text),
             *entries, data_offset, 8, 8, 16, zlib.crc32(body), 0]
    struct.pack_into("<18I", header, 0, *words)
    header[72:104] = bytes.fromhex(BASE_SHA)
    struct.pack_into("<I", header, 68, zlib.crc32(header))
    package = bytes(header) + body
    put_binary(output / "app.apx", package)
    put(output / "assets.S", '.section .app_package,"a",@progbits\n.incbin "' + wsl_path(output / "app.apx") + '"\n')
    assemble("app_loader", REPO / "firmware/ap01/app_loader.c", True)
    assemble("base", REPO / "firmware/ap01/base.c", True)
    assemble("entry", REPO / "firmware/ap01/entry.S")
    assemble("assets", output / "assets.S")
    base_symbols = link("base", ["entry", "base", "app_loader", "assets"])
    if base_symbols["ap01_entry"] != BASE_VA:
        raise ValueError("Base entry moved")
    run_wsl(prefix + "objcopy", "-O", "binary", wsl_path(output / "base.elf"), wsl_path(output / "payload.bin"))
    payload = (output / "payload.bin").read_bytes()
    if len(payload) > PAYLOAD_CAPACITY:
        raise ValueError("Payload exceeds space")
    candidate = bytearray(original)
    candidate[GIF_OFFSET:GIF_OFFSET + GIF_NEW_BYTES] = gif_new
    struct.pack_into("<I", candidate, GIF_OFFSET - 16, GIF_NEW_BYTES)
    candidate[PAYLOAD_OFFSET:PAYLOAD_OFFSET + len(payload)] = payload
    candidate[HOOK_OFFSET:HOOK_OFFSET + 6] = jal(HOOK_VA, TRAMP_VA) + bytes.fromhex("0100")
    candidate[TRAMP_OFFSET:TRAMP_OFFSET + 8] = trampoline(BASE_VA)
    refresh_recovery_crc(candidate)
    allowed = [ByteRange(GIF_OFFSET - 16, GIF_OFFSET - 12),
               ByteRange(GIF_OFFSET, GIF_OFFSET + GIF_NEW_BYTES),
               ByteRange(PAYLOAD_OFFSET, PAYLOAD_OFFSET + len(payload)),
               ByteRange(HOOK_OFFSET, HOOK_OFFSET + 6), ByteRange(TRAMP_OFFSET, TRAMP_OFFSET + 8),
               ByteRange(len(candidate) - 4, len(candidate))]
    report = validate_candidate(original, bytes(candidate), allowed)
    negative = bytearray(candidate)
    negative[0x1001] ^= 1
    refresh_recovery_crc(negative)
    try:
        validate_candidate(original, bytes(negative), allowed)
    except FirmwareValidationError:
        pass
    else:
        raise ValueError("Out-of-range mutation was not rejected")
    put_binary(output / "ap01-1.0.2_0031-app-xip.bin", candidate)
    if sha((output / "ap01-1.0.2_0031-app-xip.bin").read_bytes()) != sha(candidate):
        raise ValueError("Candidate readback mismatch")
    put(output / "symbols.json", json.dumps({"base": base_symbols, "app": app_symbols}, indent=2) + "\n")
    sources = list((REPO / "firmware/ap01").glob("*")) + [Path(__file__), REPO / "tools/ap01/verify_gif.py"]
    manifest = {"target": "AP01 1.0.2_0031 native XIP diagnostic App", "flashed": False,
                "baseline": baseline.to_dict(), "candidate": report.to_dict(),
                "allowed_ranges": [value.to_dict() for value in allowed], "toolchain": versions,
                "compiler_flags": flags, "sources_sha256": {str(p.relative_to(REPO)): sha(p.read_bytes()) for p in sources if p.is_file()},
                "package": {"bytes": len(package), "sha256": sha(package), "text_bytes": len(text),
                            "entry_offsets": entries, "initialized_bytes": 8, "bss_bytes": 8,
                            "state_bytes": 16, "malloc_bytes": 31, "abi": 1, "link_va": PACKAGE_VA},
                "payload": {"bytes": len(payload), "capacity": PAYLOAD_CAPACITY, "sha256": sha(payload),
                            "base_va": BASE_VA, "package_va": PACKAGE_VA}, "gif": gif_check,
                "validation": {"outside_range_mutation_rejected": True, "readback_sha_matches": True},
                "limitations": ["No hardware flash/boot acceptance", "No dynamic file App OTA",
                                "No full Bridge App/network authentication implementation",
                                "No crash isolation or persistent safe boot", "CRC and base SHA are not signatures"]}
    put(output / "manifest.json", json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"rom": str(output / "ap01-1.0.2_0031-app-xip.bin"),
                      "bytes": len(candidate), "sha256": sha(candidate), "package_bytes": len(package)}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--upstream", type=Path, default=REPO.parent / "cuktech-ap01-firmware-tools")
    args = parser.parse_args()
    build(args.output, args.upstream.resolve())
