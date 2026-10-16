"""Build and verify the portable AP01 maintenance core, NOT an installable ROM.

Uses the exact existing opt package read-only; never creates directories or
calls device/cloud APIs. Missing device ports always keep rom_ready false.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys

sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "firmware/ap01"
OPT_SHA = "796abb5350f3790e9184de1a0e748576f71e237365ccadd45fe315eaf1148c09"
STAGE_SHA = "24e50a65c633748f4680a59cc7e97d40582727cf27af06662fb29a6d182afa13"
MISSING_PORTS = [
    "independent maintenance executor startup and bounded HTTP transport",
    "durable binding authentication and atomic journal storage/readback",
    "vendor OTA shared exclusion and safe business stop",
    "single-copy App/Bundle installation, activation proof and original-job query",
    "maintenance screen drawing on the vendor UI thread",
]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(args, env=None):
    result = subprocess.run([str(a) for a in args], capture_output=True, text=True,
                            env=env, timeout=60)
    if result.returncode:
        raise RuntimeError(f"{Path(str(args[0])).name} failed:\n{result.stdout}{result.stderr}")
    return result.stdout


def write_new(path, value):
    with path.open("x", encoding="utf-8") as stream:
        stream.write(value)


def package_check(upstream):
    root = upstream / "artifacts/firmware/第三方固件"
    package = root / "AGENTS看板/ap01-1.0.2_0031-opt.bin"
    manifest_path = package.with_suffix(".manifest.json")
    stage = root / "物理旋钮交互优化/ap01-1.0.2_0031-opt-setting.bin"
    data, original = package.read_bytes(), stage.read_bytes()
    manifest = json.loads(manifest_path.read_text())
    if len(data) != 6804520 or sha(data) != OPT_SHA or sha(original) != STAGE_SHA:
        raise ValueError("exact existing 0031 opt/opt-setting packages required")
    if manifest["output"]["sha256"] != OPT_SHA or manifest["input"]["sha256"] != STAGE_SHA:
        raise ValueError("existing package manifest identity mismatch")
    sys.path.insert(0, str(upstream))
    from core.firmware_image import ByteRange, validate_candidate
    checked = validate_candidate(original, data, [
        ByteRange(r["start"], r["end_exclusive"]) for r in manifest["allowed_ranges"]
    ])
    return {"path": str(package), "bytes": len(data), "sha256": sha(data),
            "manifest_sha256": sha(manifest_path.read_bytes()),
            "vendor_header_and_trailer_structure_valid": checked.immutable_header_identical and checked.recovery_structure_identical,
            "recovery_crc_valid": checked.recovery.stored_crc == checked.recovery.calculated_crc,
            "personalization_required": manifest["status"] == "requires-endpoint-personalization"}


def symbols(nm, elf):
    result = {}
    for line in run([nm, "--defined-only", "-n", elf]).splitlines():
        fields = line.split()
        if len(fields) >= 3:
            result[fields[-1]] = int(fields[0], 16)
    return result


def emulate(elf, names, deps):
    sys.path.insert(0, str(deps))
    from unicorn import Uc, UC_ARCH_RISCV, UC_MODE_RISCV32
    from unicorn.riscv_const import UC_RISCV_REG_SP, UC_RISCV_REG_RA, UC_RISCV_REG_PC, UC_RISCV_REG_A0
    data = elf.read_bytes()
    if data[:6] != b"\x7fELF\x01\x01" or struct.unpack_from("<H", data, 18)[0] != 243:
        raise ValueError("ELF32 little-endian RISC-V required")
    phoff = struct.unpack_from("<I", data, 28)[0]
    phsize, count = struct.unpack_from("<HH", data, 42)
    segments = [struct.unpack_from("<8I", data, phoff + i * phsize) for i in range(count)]
    loads = [s for s in segments if s[0] == 1 and s[5]]
    low = min(s[2] for s in loads) & ~4095
    high = (max(s[2] + s[5] for s in loads) + 4095) & ~4095
    if low < 4096 or high - low > 2 * 1024 * 1024:
        raise ValueError("unexpected test ELF memory map")
    uc = Uc(UC_ARCH_RISCV, UC_MODE_RISCV32)
    uc.mem_map(low, high - low)
    for _, offset, address, _, filesz, _, _, _ in loads:
        uc.mem_write(address, data[offset:offset + filesz])
    uc.mem_map(0x90000000, 65536)
    uc.mem_map(0x800000, 4096)
    uc.reg_write(UC_RISCV_REG_SP, 0x90010000)
    uc.reg_write(UC_RISCV_REG_RA, 0x800000)
    error = None
    try:
        uc.emu_start(names["main"], 0x800000, timeout=5_000_000, count=25_000_000)
    except Exception as exc:
        error = type(exc).__name__
    line = struct.unpack("<I", bytes(uc.mem_read(names["am_test_failure"], 4)))[0]
    if error or line or uc.reg_read(UC_RISCV_REG_PC) != 0x800000 or uc.reg_read(UC_RISCV_REG_A0):
        raise RuntimeError(f"RV32 self-test failed: error={error}, assertion_line={line}")
    return {"passed": True, "scope": "compiled core + synthetic ports; no vendor RTOS/network/hardware"}


def build(args):
    output = args.output.resolve()
    if not output.is_dir() or not output.is_relative_to(REPO / "artifacts/ap01-research"):
        raise ValueError("use an existing, precreated repository AP01 artifacts directory")
    if (output / "maintenance-build.json").exists():
        raise FileExistsError("existing build evidence must not be overwritten")
    package = package_check(args.upstream.resolve())
    clang = args.llvm / "clang"
    nm = args.llvm / "llvm-nm"
    rust_root = Path(run(["rustc", "--print", "sysroot"]).strip())
    linker = rust_root / "lib/rustlib/x86_64-apple-darwin/bin/rust-lld"
    if not linker.is_file():
        raise ValueError("existing rust-lld for this macOS host required")
    env = dict(os.environ)
    env["DYLD_LIBRARY_PATH"] = str(rust_root / "lib")
    versions = {"clang": run([clang, "--version"]).splitlines()[0],
                "linker": run([linker, "-flavor", "gnu", "--version"], env).splitlines()[0]}
    common = ["--target=riscv32-unknown-elf", "-march=rv32imac", "-mabi=ilp32", "-std=c99",
              "-Oz", "-ffreestanding", "-fno-builtin", "-fno-pic", "-msmall-data-limit=0",
              "-mno-relax", "-fstack-usage", "-Wall", "-Wextra", "-Werror"]
    core = output / "maintenance.o"
    run([clang, *common, "-c", SOURCE / "maintenance.c", "-o", core])
    if run([nm, "-u", core]).strip():
        raise ValueError("maintenance core unexpectedly depends on external symbols")
    run([args.llvm / "llvm-ar", "rcs", output / "libap01-maintenance.a", core])
    linked = output / "maintenance-core.elf"
    # Generic link-check address, never an AP01 ROM injection address.
    link = [linker, "-flavor", "gnu", "-m", "elf32lriscv", "--no-undefined", "--no-relax", "-Ttext=0x10000"]
    run([*link, "--entry=am_begin", core, "-o", linked], env)
    host = output / "test-maintenance"
    run(["clang", "-std=c99", "-Wall", "-Wextra", "-Werror", "-g0",
         "-fsanitize=address,undefined", "-fno-omit-frame-pointer", SOURCE / "maintenance.c",
         SOURCE / "test_maintenance.c", "-o", host])
    host_log = run([host])
    write_new(output / "maintenance-host-tests.txt", host_log)
    test_object = output / "test-maintenance-rv32.o"
    run([clang, *common, "-DAM_RV32_SELFTEST", "-c", SOURCE / "test_maintenance.c", "-o", test_object])
    test_elf = output / "test-maintenance-rv32.elf"
    run([*link, "--entry=main", core, test_object, "-o", test_elf], env)
    rv32 = emulate(test_elf, symbols(nm, test_elf), args.deps.resolve())
    stack = {}
    for line in (output / "maintenance.su").read_text().splitlines():
        name, size, kind = line.split("\t")
        if kind != "static":
            raise ValueError("dynamic stack use is not allowed")
        stack[name.rsplit(":", 1)[-1]] = int(size)
    size_log = run([args.llvm / "llvm-size", linked])
    fields = size_log.splitlines()[-1].split()
    if int(fields[1]) or int(fields[2]):
        raise ValueError("core must not have writable global data")
    chain = max(stack["am_receive"] + stack["fail"] + stack["save"] + stack["am_encode"],
                stack["am_commit"] + stack["result"] + stack["save"] + stack["am_encode"])
    write_new(output / "maintenance-core.dis", run([args.llvm / "llvm-objdump", "-d", linked]))
    if sha(Path(package["path"]).read_bytes()) != OPT_SHA:
        raise ValueError("upstream input changed during build")
    outputs = [core, linked, output / "libap01-maintenance.a", test_elf]
    report = {"artifact_kind": "portable-core-build", "rom_ready": False,
              "device_service_ready": False, "device_ui_ready": False, "flashed": False,
              "missing_device_ports": MISSING_PORTS, "upstream_package": package,
              "tools": versions, "host_tests": host_log.strip(), "rv32_tests": rv32,
              "resources": {"rv32_text_bytes": int(fields[0]), "data_bytes": 0, "bss_bytes": 0,
                            "state_bytes": 136, "request_bytes": 100,
                            "persistent_record_bytes": {"empty": 72, "principal32": 104, "principal64": 136},
                            "core_stack_chain_bytes": chain, "adapter_stack_bytes": None,
                            "stack_frames": stack, "dynamic_allocations": 0},
              "sources": {p.name: sha(p.read_bytes()) for p in [SOURCE / "maintenance.h", SOURCE / "maintenance.c", SOURCE / "test_maintenance.c", Path(__file__)]},
              "outputs": {p.name: {"bytes": p.stat().st_size, "sha256": sha(p.read_bytes())} for p in outputs}}
    write_new(output / "maintenance-build.json", json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"core_build": "passed", "rv32_tests": "passed", "rom_ready": False,
                      "report": str(output / "maintenance-build.json")}, ensure_ascii=False))
    return 2 if args.require_rom else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--upstream", type=Path, default=REPO.parent / "cuktech-ap01-firmware-tools")
    parser.add_argument("--llvm", type=Path, default=Path("/usr/local/opt/llvm/bin"))
    parser.add_argument("--deps", type=Path, default=REPO / "artifacts/ap01-research/ble0031-20261014/deps")
    parser.add_argument("--require-rom", action="store_true", help="exit 2 if a deployable ROM cannot be produced")
    args = parser.parse_args()
    try:
        return build(args)
    except (OSError, ValueError, RuntimeError) as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
