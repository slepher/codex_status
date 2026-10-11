"""Read-only, exact-baseline AP01 evidence extraction. No firmware is produced."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import sys
from pathlib import Path

import capstone

PROJECT = Path(__file__).resolve().parents[2]
# Read-only upstream inputs; research code and outputs belong to codex_status.
ROOT = PROJECT.parent / "cuktech-ap01-firmware-tools"
sys.path.insert(0, str(ROOT))
from core.firmware_image import validate_baseline, FirmwareValidationError

DELTA = 0x9FFFF000
IMAGE = ROOT / "artifacts/firmware/original/ap01-1.0.2_0031.bin"
# Bounds are research windows, not inferred complete function extents.
WINDOWS = {
    "reset": (0xA0000000, 0xA0000052),
    "heap_initial": (0xA0028EF4, 0xA0028F20),
    "heap_extensions": (0xA0029540, 0xA002958C),
    "heap_addregion": (0xA002E914, 0xA002E9CC),
    "heap_initialize": (0xA002E9CC, 0xA002EAC4),
    "malloc_shared_heap": (0xA002FC1A, 0xA002FC4A),
    "loader_malloc_wrapper": (0xA007E1C4, 0xA007E1EE),
    "board_data_partition": (0xA0055894, 0xA0055A18),
    "mtd_partition": (0xA00522F2, 0xA0052428),
    "flash_geometry": (0xA0057540, 0xA00575EC),
    "partition_table_validation": (0xA00575EC, 0xA005767A),
    "flash_read": (0xA0057EAA, 0xA0057F7A),
    "partition_init": (0xA0058F2A, 0xA00593CA),
    "register_mtd": (0xA019C97C, 0xA019C9E2),
    "nsh_mount_and_startup": (0xA0032182, 0xA0032348),
    "builtin_index_fields": (0xA0031B84, 0xA0031BC8),
    "stock_ui_task_entry": (0xA0072BBE, 0xA0072C7C),
    "ota_signed_file_branch": (0xA006851E, 0xA00687AE),
    "reset_callee": (0xA0029A1E, 0xA0029D74),
    "timer_init_schedule": (0xA009B376, 0xA009B468),
    "timer_cancel": (0xA01AB0C2, 0xA01AB218),
    "ui_creation": (0xA00B2420, 0xA00B2850),
    "weather_callback": (0xA00B6CA6, 0xA00B7000),
    "dispatch_accessors": (0xA00BE388, 0xA00BE3E4),
    "switch_page": (0xA00BFA4E, 0xA00BFABC),
    "webclient_entry": (0xA00D86BA, 0xA00D8840),
    "power_data_lifetime": (0xA00AB684, 0xA00AB6C0),
    "power_confirm": (0xA00BC8C6, 0xA00BC9E0),
}
STRINGS = [
    "BL618", "NuttX", "jd9853", "/dev/input0", "/dev/fb0",
    "/dev/ttyS0", "/dev/ttyS1", "/dev/ttyS2", "/dev/fw1", "/dev/fw2",
    "/dev/ota", "/dev/data", "/dev/resource", "/dev/reserve", "/dev/efuse",
    "signed_file", "original_length", "app_force", "cert_verify",
    "downgrade disabled", "unsigned app firmware rejected", "miIO.ota",
    "PogoPinMng/UartComm.c", "UsbIfaceMng.c", "UsbIfaceOta.c", "USB MUX",
    "usb20", "usbdev", "usbhost", "cdc_acm", "ttyACM", "ttyUSB", "BFPT",
    "ro.boot.current_slot", "ro.boot.update_slot", "ro.boot.slot_count",
    "ap01_boot_header_crc", "BL616 partition magic error", "NSH auto exit.",
    "nsh_main", "miio_client", "lvgl_ui_main", "mible_mesh_common_bt",
    "/dev/crash", "socket/listen.c", "local/local_listen.c", "nsh_consolemain.c",
]
KNOWN = {
    "partition_init": 0xA0058F2A, "mtd_partition": 0xA00522F2,
    "flash_geometry": 0xA0057540, "register_mtd": 0xA019C97C,
    "flash_read": 0xA0057EAA, "builtin_table": 0xA01B9894,
    "startup_volume": 0xA01B9DA0, "stock_ui_task_entry": 0xA0072BBE,
    "stock_ui_create": 0xA00B2420, "stock_key_event": 0xA00BBFEE,
    "stock_dispatch": 0xA00BE388, "stock_child": 0xA00BE3CA,
    "stock_switch": 0xA00BFA4E, "timer_init": 0xA009B376,
    "timer_schedule": 0xA009B3B6, "timer_cancel": 0xA01AB0C2,
    "webclient_perform": 0xA00D86BA,
    "weather_timer": 0x62FCA9A8, "port_count": 0x62FC4CD8,
    "power_data_pointer": 0x62FCAA00,
}

def skip_unknown(code, size, offset, user_data):
    half = int.from_bytes(bytes(code[offset:offset+2]), "little")
    width = 2 if half & 3 != 3 else 4
    return width if offset + width <= size else 0

def disassembler():
    md = capstone.Cs(capstone.CS_ARCH_RISCV,
                     capstone.CS_MODE_RISCV32 | capstone.CS_MODE_RISCVC)
    md.skipdata = True
    md.skipdata_setup = ("undecoded_instruction", skip_unknown, None)
    return md

def decode(data, address):
    return list(disassembler().disasm_lite(data, address))

def static_edge(previous, current):
    """Resolve only direct relative jumps and adjacent address materialization."""
    pc, size, mnemonic, operands = current
    fields = [p.strip() for p in operands.split(",")]
    try:
        if mnemonic in ("jal", "j", "c.jal", "c.j"):
            return ((pc + int(fields[-1], 0)) & 0xFFFFFFFF, "direct_jump")
        if not previous:
            return None
        pp, ps, pm, po = previous
        if pp + ps != pc or pm not in ("lui", "auipc", "c.lui"):
            return None
        reg, imm = [p.strip() for p in po.split(",")]
        base = (int(imm, 0) << 12) + (pp if pm == "auipc" else 0)
        if mnemonic == "addi" and fields[1] == reg:
            return ((base + int(fields[2], 0)) & 0xFFFFFFFF, "address_pair")
        if mnemonic == "c.addi" and fields[0] == reg:
            return ((base + int(fields[1], 0)) & 0xFFFFFFFF, "address_pair")
        if mnemonic == "jalr" and len(fields) == 3 and fields[1] == reg:
            return ((base + int(fields[2], 0)) & 0xFFFFFFFE, "indirect_call_pair")
        if mnemonic in ("lw", "lbu", "lh", "lhu", "lb", "jalr"):
            match = re.search(r"(-?0x[0-9a-f]+|-?\d+)\((\w+)\)", operands)
            if match and match[2] == reg:
                target = (base + int(match[1], 0)) & 0xFFFFFFFF
                return (target & ~1 if mnemonic == "jalr" else target,
                        "indirect_call_pair" if mnemonic == "jalr" else "address_pair")
            if mnemonic == "jalr" and fields[-1] == reg:
                return (base & 0xFFFFFFFE, "indirect_call_pair")
    except (ValueError, IndexError):
        return None
    return None

def find_all(data, needle):
    start = 0
    while (pos := data.find(needle, start)) >= 0:
        yield pos
        start = pos + 1

def containing_string_start(data, pos, lookback=160):
    boundary = data.rfind(b"\0", max(0, pos-lookback), pos)
    if boundary >= 0:
        return boundary + 1
    return 0 if pos < lookback else None


def startup_script(data):
    """Validate the known 0031 ROMFS and one fixed rcS entry, not a general unpacker."""
    start, size, entry = 0x1BADA0, 0x1E0, 0x150
    volume = data[start:start+size]
    if len(volume) != size or volume[:8] != b"-rom1fs-":
        raise ValueError("0031 startup ROMFS missing or truncated")
    if struct.unpack_from(">I", volume, 8)[0] != size:
        raise ValueError("unexpected startup ROMFS length")
    if sum(struct.unpack(">120I", volume)) & 0xFFFFFFFF:
        raise ValueError("startup ROMFS checksum mismatch")
    if volume[16:27] != b"NSHInitVol\0":
        raise ValueError("unexpected startup volume name")
    header = volume[entry:entry+32]
    next_type, info, length, checksum = struct.unpack_from(">4I", header)
    if (next_type, info, length) != (2, 0, 97) or header[16:20] != b"rcS\0":
        raise ValueError("unexpected rcS file header")
    if sum(struct.unpack(">8I", header)) & 0xFFFFFFFF:
        raise ValueError("rcS header checksum mismatch")
    script = volume[entry+32:entry+32+length].decode("ascii")
    return {"volume_offset": hex(start), "volume_size": size,
            "header_offset": hex(start+entry), "data_offset": hex(start+entry+32),
            "length": length, "checksum": "volume and rcS header sums equal zero",
            "script": script, "meaning": "embedded startup script; physical console access unproven"}


def builtin_inventory(data):
    rows = []
    for offset in range(0x1BA894, 0x1BA914, 16):
        name_va, priority, stack, entry = struct.unpack_from("<4I", data, offset)
        start = name_va - DELTA
        if not 0 <= start < len(data):
            raise ValueError("builtin name outside image")
        end = data.find(b"\0", start, start+80)
        if end < 0 or not 0xA0000000 <= entry < 0xA01AF000:
            raise ValueError("invalid builtin record")
        rows.append({"file_offset": hex(offset), "name": data[start:end].decode("ascii"),
                     "priority": priority, "configured_stack_bytes": stack,
                     "entry_va": hex(entry), "raw_record": data[offset:offset+16].hex()})
    return rows


def symbol_inventory():
    rows = []
    paths = [ROOT / "features/agents_dashboard_firmware/result_loader.c",
             ROOT / "features/agents_dashboard_firmware/result_loader.ld"]
    patterns = [
        r"^#define\s+(VA_\w+)\s+(0x[0-9a-fA-F]+)",
        r"PROVIDE\((\w+)\s*=\s*(0x[0-9a-fA-F]+)\)",
    ]
    for path in paths:
        for line, text in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for pattern in patterns:
                m = re.search(pattern, text)
                if m:
                    rows.append({"name": m[1], "va": m[2].lower(),
                                 "source": path.relative_to(ROOT).as_posix(), "line": line,
                                 "meaning": "existing_source_label_not_new_ABI_proof"})
    return rows

def self_test(data):
    validate_baseline(data)
    mutated = bytearray(data)
    mutated[0x1000] ^= 1
    try:
        validate_baseline(bytes(mutated))
    except FirmwareValidationError:
        pass
    else:
        raise AssertionError("changed baseline accepted")
    first = decode(data[0x1000:0x1014], 0xA0000000)
    assert static_edge(first[0], first[1])[0] == 0x62FDB890
    assert static_edge(first[3], first[4])[0] == 0x62FC34C0
    jump = decode(bytes.fromhex("ef000001"), 0x1000)[0]
    assert static_edge(None, jump) == (0x1010, "direct_jump")
    assert static_edge((0,4,"lui","a0, 0xa0001"),
                       (4,4,"addi","a0, a0, -0x800"))[0] == 0xA0000800
    assert static_edge((0xA0000000,4,"auipc","ra, 0x10"),
                       (0xA0000004,4,"jalr","ra, ra, -0x10"))[0] == 0xA000FFF0
    unknown = decode(bytes.fromhex("0b4647588280"), 0x1000)
    assert unknown[0][1:3] == (4, "undecoded_instruction")
    assert unknown[1][0] == 0x1004 and unknown[1][2] == "c.jr"
    schedule = decode(data[0x9C3B6:0x9C468], 0xA009B3B6)
    assert any(m in ("sw", "c.sw") and "0x54(" in op for _,_,m,op in schedule)
    assert static_edge(None, (0x1000, 4, "j", "-0x10")) == (0xFF0, "direct_jump")
    assert containing_string_start(b"old\0prefix needle\0", 11) == 4
    assert containing_string_start(b"prefix needle\0", 7) == 0
    assert containing_string_start(b"a"*200, 190) is None
    builtins = {row["name"]: row for row in builtin_inventory(data)}
    assert len(builtins) == 8
    assert builtins["lvgl_ui_main"]["configured_stack_bytes"] == 65536
    assert builtins["lvgl_ui_main"]["entry_va"] == "0xa0072bbe"
    script = startup_script(data)
    assert script["script"].endswith('echo "NSH auto exit."\nexit\n')
    changed = bytearray(data)
    changed[0x1BAF10] ^= 1
    try:
        startup_script(bytes(changed))
    except ValueError:
        pass
    else:
        raise AssertionError("damaged ROMFS accepted")
    print("self-test: baseline, mutation rejection, PC/sign arithmetic, jumps, timer, string boundaries, ROMFS integrity, builtin table PASS")

def analyze(data):
    baseline = validate_baseline(data).to_dict()
    symbols = symbol_inventory()
    targets = {value: name for name, value in KNOWN.items()}
    for item in symbols:
        targets.setdefault(int(item["va"], 16), item["name"])
    strings = {}
    for term in STRINGS:
        positions = list(find_all(data, term.encode()))
        strings[term] = [hex(p) for p in positions]
        for pos in positions:
            targets.setdefault(pos + DELTA, "string:" + term)
            start = containing_string_start(data, pos)
            if start is not None:
                targets.setdefault(start + DELTA, "containing_string:" + term)
    references = []
    previous = None
    undecoded = 0
    count = 0
    # A bounded linear sweep; mixed data and unsupported vendor opcodes are possible.
    for insn in disassembler().disasm_lite(data[0x1000:0x1B0000], 0xA0000000):
        count += 1
        undecoded += insn[2] == "undecoded_instruction"
        edge = static_edge(previous, insn)
        if edge and edge[0] in targets:
            references.append({"at": hex(insn[0]), "target": hex(edge[0]),
                               "label": targets[edge[0]], "kind": edge[1]})
        previous = insn
    pointers = {}
    for address, label in targets.items():
        hits = list(find_all(data, struct.pack("<I", address)))
        if hits:
            pointers[hex(address)] = {"label": label, "file_offsets": [hex(p) for p in hits],
                                     "meaning": "byte_matches_not_proven_pointer_table"}
    text = []
    for name, (start, end) in WINDOWS.items():
        text.append(f"\n[{name}] VA {start:#x}..{end:#x}, file {start-DELTA:#x}..{end-DELTA:#x}")
        for pc, size, mnemonic, operands in decode(data[start-DELTA:end-DELTA], start):
            raw = data[pc-DELTA:pc-DELTA+size].hex()
            text.append(f"{pc:08x} {raw:12s} {mnemonic:22s} {operands}")
    evidence = {
        "baseline": baseline, "capstone": capstone.__version__,
        "address_rule": "VA=file_offset+0x9ffff000; not a physical flash write address",
        "startup": {"sp": "0x62fdb890", "gp": "0x62fc34c0"},
        "payload": {"start": "0x2475d0", "end_exclusive": hex(0x1F80AC+385834),
                    "capacity": 0x1F80AC+385834-0x2475D0,
                    "meaning": "after existing GIF optimization, not original empty bytes"},
        "startup_romfs": startup_script(data),
        "builtin_applications": builtin_inventory(data),
        "source_symbols": symbols, "string_offsets": strings,
        "candidate_references": references, "raw_pointer_matches": pointers,
        "scan": {"start": "0xa0000000", "file_end_exclusive": "0x1b0000",
                 "instructions_or_unknowns": count, "undecoded_instructions": undecoded,
                 "limitations": "linear sweep; no complete CFG/function boundaries; indirect calls omitted; raw bytes may be data; unsupported 32-bit instructions skipped as 4 bytes"},
    }
    return evidence, "\n".join(text) + "\n"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--output", type=Path, help="existing output directory; never overwrite")
    args = parser.parse_args()
    data = IMAGE.read_bytes()
    if args.self_test:
        self_test(data)
    if args.output:
        if not args.output.is_dir():
            parser.error("output directory must already exist")
        outputs = [args.output / "evidence.json", args.output / "selected-disassembly.txt"]
        if any(p.exists() for p in outputs):
            parser.error("output exists; choose an empty evidence directory")
        evidence, assembly = analyze(data)
        outputs[0].write_text(json.dumps(evidence, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
        outputs[1].write_text(assembly, encoding="utf-8")
        assert hashlib.sha256(IMAGE.read_bytes()).hexdigest() == evidence["baseline"]["sha256"]
        print(json.dumps({"references": len(evidence["candidate_references"]),
                          "symbols": len(evidence["source_symbols"]),
                          "scan": evidence["scan"], "outputs": [str(p) for p in outputs]}))
    if not args.self_test and not args.output:
        parser.error("choose --self-test or --output")

if __name__ == "__main__":
    main()
