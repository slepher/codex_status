"""Read-only production-layout sizing; builds a data-only RV32 object, never a ROM."""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import struct
import subprocess
from pathlib import Path

PROJECT = Path(__file__).resolve().parents[2]
# Read-only upstream inputs; research code and outputs belong to codex_status.
ROOT = PROJECT.parent / "cuktech-ap01-firmware-tools"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def linux_path(path):
    path = path.resolve()
    return "/mnt/" + path.drive[0].lower() + path.as_posix()[2:]

def run(*args):
    try:
        return subprocess.check_output(list(map(str, args)), text=True, encoding="utf-8", stderr=subprocess.STDOUT)
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(exc.output) from exc

def source_struct(text, name):
    found = re.search(r"struct " + re.escape(name) + r" \{.*?\n\};", text, re.S)
    if not found:
        raise ValueError("production structure not found: " + name)
    return found[0]

def calculate(ct, slot_header, meta):
    rows = []
    cache = 17 + 4 + 12 + ct
    for label, n, payload in [("empty", 0, 0), ("one_32KiB", 1, 32768),
                              ("typical_four_128KiB", 4, 131072),
                              ("upper_eight_256KiB", 8, 262144)]:
        if not n:
            rows.append(dict(case=label, templates=0, payload_bytes=0,
                             steady_bytes=0, install_peak_bytes=0,
                             current_preflight_required_total=0, proposed_ab_peak_bytes=0))
            continue
        slot = slot_header + payload + cache * n
        stable = 2 * slot + 2 * meta
        # Assumes an old and a prior inactive slot of the same size exist.
        # Current preflight asks for an extra slot BEFORE truncating inactive.
        guard = stable + payload + slot + 4096
        # Proposed AP01 file: 64-byte explicit header + exact source payload +
        # 32-byte index per item + existing serialized compiled cache per item.
        proposed_slot = 64 + payload + n * (32 + 12 + ct)
        rows.append(dict(case=label, templates=n, payload_bytes=payload,
                         slot_bytes=slot, steady_bytes=stable,
                         install_peak_bytes=stable + payload,
                         current_preflight_required_total=guard,
                         proposed_slot_bytes=proposed_slot,
                         proposed_ab_peak_bytes=2*proposed_slot + 512 + 65536))
    return rows

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--output", type=Path, required=True)
    args = ap.parse_args()
    out = args.output.resolve()
    if not out.is_dir(): ap.error("output directory must already exist")
    outputs = [out / name for name in ("layout-probe.c", "layout-probe.o", "layout-probe.bin", "resource-budget.json", "layout-probe.s")]
    if any(p.exists() for p in outputs): ap.error("output exists; refusing overwrite")
    header_path = PROJECT / "src/template_engine.h"
    bundle_header_path = PROJECT / "src/bundle_store.h"
    bundle_path = PROJECT / "src/bundle_store.cpp"
    header = header_path.read_text(encoding="utf-8")
    bundle_header = bundle_header_path.read_text(encoding="utf-8")
    bundle = bundle_path.read_text(encoding="utf-8")
    defs = "\n".join(re.findall(r"^#define (?:CT_|BS_).*", header + "\n" + bundle_header, re.M))
    types = ["CtReq", "CtOp", "CtResource", "CtTemplate", "SlotHeader", "MetaRecord"]
    body = "\n".join(source_struct(header if t.startswith("Ct") else bundle, t) for t in types)
    probe = ("typedef unsigned char uint8_t;\ntypedef unsigned short uint16_t;\n"
             "typedef short int16_t;\ntypedef unsigned int uint32_t;\n" + defs + "\n" +
             "\n".join("typedef struct " + t + " " + t + ";" for t in types) +
             "\n#pragma pack(push, 1)\n" + body + "\n#pragma pack(pop)\n" +
             '_Static_assert(sizeof(void*) == 4 && sizeof(uint32_t) == 4, "RV32 required");\n' +
             '__attribute__((section(".budget"),used)) const uint32_t sizes[] = {0x42554447,' +
             ",".join("sizeof("+t+")" for t in types) + "};\n")
    outputs[0].write_text(probe, encoding="utf-8", newline="\n")
    bin_dir = ROOT / "tools/docker/toolchain/bin"
    compiler = linux_path(bin_dir / "riscv64-unknown-elf-gcc")
    copier = linux_path(bin_dir / "riscv64-unknown-elf-objcopy")
    version = run("wsl", "-d", "Ubuntu-24.04", "--", compiler, "--version").splitlines()[0]
    command = ["wsl", "-d", "Ubuntu-24.04", "--", compiler, "-B" + linux_path(bin_dir) + "/", "-march=rv32imac", "-mabi=ilp32",
               "-std=c11", "-Os", "-ffreestanding", "-fno-common", "-S", linux_path(outputs[0]), "-o", linux_path(outputs[4])]
    run(*command)
    run("wsl", "-d", "Ubuntu-24.04", "--", linux_path(bin_dir / "riscv64-unknown-elf-as"), "-march=rv32imac", "-mabi=ilp32", linux_path(outputs[4]), "-o", linux_path(outputs[1]))
    run("wsl", "-d", "Ubuntu-24.04", "--", copier, "-O", "binary", "-j", ".budget", linux_path(outputs[1]), linux_path(outputs[2]))
    values = struct.unpack("<7I", outputs[2].read_bytes())
    assert values[0] == 0x42554447
    sizes = dict(zip(types, values[1:]))
    assert sizes["CtTemplate"] == 25 + sizes["CtOp"]*64 + sizes["CtReq"]*32 + sizes["CtResource"]*16
    assert sizes["SlotHeader"] == 130 and sizes["MetaRecord"] == 138
    rows = calculate(sizes["CtTemplate"], sizes["SlotHeader"], sizes["MetaRecord"])
    assert rows[0]["steady_bytes"] == 0
    assert all(x["current_preflight_required_total"] >= x["install_peak_bytes"] for x in rows)
    assert rows[-1]["proposed_ab_peak_bytes"] < rows[-1]["install_peak_bytes"]
    # Planning allowances, not target measurements. Font bytes are additional.
    steady = {"active_compiled": sizes["CtTemplate"], "two_mono_frames": 2*9600,
              "rgb565_line_16": 10240, "rx_tx_frames": 2*4096,
              "http_headers": 1024, "replay_response": 512,
              "protocol_state_allowance": 4096, "usage_and_fields_allowance": 2*8192,
              "json_scratch_allowance": 32768, "service_stack_allowance": 12288,
              "file_buffers_allowance": 8192}
    paths = [header_path,bundle_header_path,bundle_path,
             PROJECT/'src/template_engine.cpp',PROJECT/'src/font_asset.h',
             PROJECT/'bridge/crates/core/src/platform/model.rs',
             PROJECT/'bridge/crates/core/src/platform/service.rs']
    doc = dict(kind="resource-budget-not-a-firmware", compiler=version, command=command,
               source_sha256={str(p.relative_to(PROJECT)):digest(p) for p in paths},
               sizeof_rv32=sizes, serialized_compiled_bytes=12+sizes["CtTemplate"],
               storage_scenarios=rows, scenario_note="Payload sizes are declared planning cases, not measurements of user data. Storage excludes stock files and filesystem overhead except explicit reserves.",
               proposed_steady_allowance=steady, proposed_steady_base_sum=sum(steady.values()),
               ram_note="Planning sum includes fixed-size current CompiledTemplate and bounded staging; font assets, stock heap usage, socket/TLS/BLE deltas and allocator overhead are additional. No device free-heap measurement.",
               generated_sha256={p.name:digest(p) for p in outputs[:3]+outputs[4:]})
    outputs[3].write_text(json.dumps(doc,ensure_ascii=False,indent=2)+"\n",encoding="utf-8",newline="\n")
    print(json.dumps({"sizes":sizes,"storage":rows,"steady_base_allowance":sum(steady.values()),"report":str(outputs[3])},ensure_ascii=False))

if __name__ == "__main__": main()
