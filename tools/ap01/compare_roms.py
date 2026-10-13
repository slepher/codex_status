"""Exact 0041/0031 static comparison. Produces research evidence, never a ROM."""
from __future__ import annotations
import argparse
from bisect import bisect_left
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import struct
import sys
import zlib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ap01_0031", Path(__file__).with_name("__main__.py"))
old = importlib.util.module_from_spec(spec)
spec.loader.exec_module(old)
DELTA = 0x9ffff000
INPUTS = {
    "0041": (ROOT / "artifacts/ap01-research/ap01-1.0.2_0041.bin", 6769704,
             "972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9"),
    "0031": (ROOT.parent / "cuktech-ap01-firmware-tools/artifacts/firmware/original/ap01-1.0.2_0031.bin", 6804520,
             "8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb"),
}
KEYWORDS = old.STRINGS + [
    "unsigned app firmware rejected for security", "signed app", "signature", "signed_file",
    "sha256", "ecdsa", "rsa", "public key", "pk_verify", "verify_hash", "app_url", "file_md5",
    "ota_install", "miIO.get_ota_state", "miIO.get_ota_progress", "cert_verify", "original_length",
    "app_force", "downgrade", "original_size", "firmware", "recovery", "PogoPin", "UartComm",
    "UsbIface", "usb", "USB", "HTTP/1.", "listen", "accept", "socket", "bind failed",
    "127.0.0.1", "webclient", "iot-ota", "/resource", "littlefs", "ROMFS", "NSHInitVol",
    "mallinfo", "mm_addregion", "psram", "PSRAM", "lv_disp", "lvgl", "jd9853", "task_create",
    "timer_create", "pthread", "nsh_main", "bl616_media", "ap01_boot_header_crc", "ro.boot",
    "uncompress", "inflate", "lodepng", "1.0.2_0041", "njcuk.enstor.ap01", "wlan0",
]


def hexbytes(data, address, length=32):
    offset = address - DELTA
    return data[max(offset, 0):max(offset, 0) + length].hex()


def strings(data):
    found = {}
    for key in dict.fromkeys(KEYWORDS):
        rows = []
        for pos in old.find_all(data, key.encode()):
            start = old.containing_string_start(data, pos, lookback=1024)
            if start is None:
                continue
            end = data.find(b"\0", pos)
            if end < 0 or end - start > 2048:
                continue
            value = data[start:end].decode("utf-8", errors="replace")
            row = {"offset": start, "va": hex(DELTA + start), "matched_offset": pos,
                   "matched_va": hex(DELTA + pos), "value": value}
            if row not in rows:
                rows.append(row)
        found[key] = rows
    return found


def inventory(version, data):
    candidates = [pos for pos in old.find_all(data, b"-rom1fs-")
                  if data[pos + 8:pos + 12] == struct.pack(">I", 480)
                  and data[pos + 16:pos + 27] == b"NSHInitVol\0"]
    if len(candidates) != 1:
        raise ValueError("expected one startup ROMFS")
    volume_start = candidates[0]
    size = struct.unpack_from(">I", data, volume_start + 8)[0]
    if size != 480:
        raise ValueError("unexpected startup ROMFS size")
    volume = data[volume_start:volume_start + size]
    if sum(struct.unpack(">120I", volume)) & 0xffffffff:
        raise ValueError("startup ROMFS checksum mismatch")
    name = volume.find(b"rcS\0")
    entry = name - 16
    header = volume[entry:entry + 32]
    if name < 16 or struct.unpack_from(">3I", header) != (2, 0, 97):
        raise ValueError("unexpected rcS entry")
    if sum(struct.unpack(">8I", header)) & 0xffffffff:
        raise ValueError("rcS header checksum mismatch")
    rows = []
    table = {"0031": 0x1ba894, "0041": 0x1be934}[version]
    for offset in range(table, table + 128, 16):
        pointer, priority, stack, entry_va = struct.unpack_from("<4I", data, offset)
        start = pointer - DELTA
        if not 0 <= start < len(data) or not 0xa0000000 <= entry_va < 0xa01bd7a0:
            raise ValueError("invalid builtin pointer")
        end = data.find(b"\0", start, start + 80)
        if end < 0:
            raise ValueError("unterminated builtin name")
        rows.append({"file_offset": hex(offset), "name": data[start:end].decode("ascii"),
                     "priority": priority, "configured_stack_bytes": stack,
                     "entry_va": hex(entry_va), "raw_record": data[offset:offset+16].hex()})
    if data[table + 128:table + 144] != bytes(16):
        raise ValueError("missing builtin terminator")
    return {"version": version, "sha256": hashlib.sha256(data).hexdigest(),
            "romfs_offset": hex(volume_start), "romfs_size": size,
            "romfs_checksum": "volume and rcS header sums equal zero",
            "script_offset": hex(volume_start + entry + 32),
            "script": volume[entry + 32:entry + 32 + 97].decode("ascii"), "builtins": rows}


def analyze(version):
    path, size, digest = INPUTS[version]
    data = path.read_bytes()
    if len(data) != size or hashlib.sha256(data).hexdigest() != digest:
        raise ValueError("input identity mismatch: " + version)
    if data[:4] != b"BFNP" or data[8:12] != b"FCFG" or data[0x64:0x68] != b"PCFG":
        raise ValueError("image header markers mismatch")
    if zlib.crc32(data[:252]) & 0xffffffff != struct.unpack_from("<I", data, 252)[0]:
        raise ValueError("boot header CRC mismatch")
    trailer = len(data) - 40
    crc = (zlib.crc32(data[:-4], 0xffffffff) ^ 0xffffffff) & 0xffffffff
    if data[trailer:trailer+24] != b"0x5245434f56455259544147":
        raise ValueError("recovery tag mismatch")
    if struct.unpack_from(">I", data, trailer+32)[0] != size or struct.unpack_from("<I", data, trailer+36)[0] != crc:
        raise ValueError("recovery length/CRC mismatch")
    values = strings(data)
    # GCC suffix pooling can point inside another string, e.g. mcu_signed_file + 4.
    targets = {int(row[field], 16) for rows in values.values() for row in rows
               for field in ("va", "matched_va")}
    xrefs = {hex(target): [] for target in targets}
    # Excludes string/application-table region and graphics, not an inferred section map.
    code_end = min(row["offset"] for row in values["lvgl_ui_main"] if row["offset"] > 0x100000)
    gp = (0xa000000c + (int.from_bytes(data[0x100c:0x1010], "little") & 0xfffff000)
          + ((int.from_bytes(data[0x1010:0x1014], "little") >> 20) - 0x1000
             if int.from_bytes(data[0x1010:0x1014], "little") >> 31
             else int.from_bytes(data[0x1010:0x1014], "little") >> 20)) & 0xffffffff
    count = 0
    unknown = 0
    registers = {}
    previous = None
    all_ins = []
    for ins in old.disassembler().disasm_lite(data[0x1000:code_end], 0xa0000000):
        pc, width, mnemonic, operands = ins
        count += 1
        unknown += mnemonic == "undecoded_instruction"
        all_ins.append(ins)
        edge = old.static_edge(previous, ins)
        fields = [s.strip() for s in operands.split(",")]
        registers = {r: item for r, item in registers.items() if pc - item[1] <= 32}
        registers["gp"] = (gp, pc)
        resolved = []
        if edge:
            resolved.append((edge[0], edge[1]))
        try:
            if mnemonic in ("lui", "c.lui", "auipc"):
                registers[fields[0]] = (((int(fields[1], 0) << 12) + (pc if mnemonic == "auipc" else 0)) & 0xffffffff, pc)
            elif mnemonic in ("addi", "c.addi"):
                dest, source, imm = (fields[0], fields[1], fields[2]) if mnemonic == "addi" else (fields[0], fields[0], fields[1])
                value = registers.get(source)
                if value:
                    result = (value[0] + int(imm, 0)) & 0xffffffff
                    registers[dest] = (result, pc)
                    resolved.append((result, "bounded-register-candidate"))
                else:
                    registers.pop(dest, None)
            elif mnemonic in ("lw", "lbu", "lh", "lhu", "lb", "sw", "c.lw", "c.sw"):
                match = re.search(r"(-?0x[0-9a-f]+|-?\d+)\((\w+)\)", operands)
                if match and match[2] in registers:
                    result = (registers[match[2]][0] + int(match[1], 0)) & 0xffffffff
                    resolved.append((result, "bounded-memory-operand-candidate"))
                if mnemonic not in ("sw", "c.sw"):
                    registers.pop(fields[0], None)
            elif mnemonic not in ("nop", "c.nop", "sw", "sh", "sb", "c.swsp"):
                # Conservative invalidation; this is a linear candidate scan, not CFG dataflow.
                if fields and fields[0] in registers:
                    registers.pop(fields[0], None)
        except (ValueError, IndexError):
            pass
        for target, kind in dict.fromkeys(resolved):
            if target in targets:
                xrefs[hex(target)].append({"pc": hex(pc), "kind": kind,
                    "instruction": mnemonic + " " + operands, "bytes": hexbytes(data, pc, width)})
        if mnemonic in ("ret", "c.jr", "jal", "jalr", "c.jal", "c.jalr", "j", "c.j") or mnemonic.startswith("b"):
            registers.clear()
        previous = ins
    for target in targets:
        pointer = struct.pack("<I", target)
        for pos in old.find_all(data, pointer):
            xrefs[hex(target)].append({"offset": hex(pos), "kind": "unaligned-raw-pointer-candidate"})
    result = {
        "version": version, "path": str(path), "size": size, "sha256": digest,
        "md5": hashlib.md5(data).hexdigest(), "address_rule": "VA = file offset + 0x9ffff000; not a flash burn address",
        "header_256_hex": data[:256].hex(), "header_crc_calculated": hex(zlib.crc32(data[:252]) & 0xffffffff),
        "header_crc_stored": hex(struct.unpack_from("<I", data, 252)[0]),
        "recovery": {"offset": hex(trailer), "crc": hex(crc), "length_verified": True},
        "startup_gp": hex(gp), "scan": {"offset_start": "0x1000", "offset_end_exclusive": hex(code_end),
            "instructions": count, "undecoded_items_including_possible_data": unknown,
            "coverage": "linear candidate references, not full call graph or vendor-opcode count"},
        "strings": values, "candidate_xrefs": xrefs,
    }
    return result, data, all_ins


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--inventory-only", action="store_true")
    args = parser.parse_args()
    if not args.output.is_dir():
        parser.error("precreate output directory")
    if args.self_test:
        old.self_test(INPUTS["0031"][0].read_bytes())
        for version in INPUTS:
            evidence, data, _ = analyze(version)
            assert evidence["header_crc_calculated"] == evidence["header_crc_stored"]
            assert evidence["startup_gp"] == "0x62fc34c0"
            signed = {row["matched_va"] for row in evidence["strings"]["signed_file"]}
            assert any(evidence["candidate_xrefs"][va] for va in signed)
            stock = inventory(version, data)
            assert stock["builtins"][0]["configured_stack_bytes"] == 65536
            if version == "0031":
                assert stock["builtins"] == old.builtin_inventory(data)
                assert stock["script"] == old.startup_script(data)["script"]
        print("Both exact baselines, boot/recovery CRC, startup GP and suffix-pooled references PASS")
        return
    if args.inventory_only:
        result = {}
        for version in INPUTS:
            _, data, _ = analyze(version)
            result[version] = inventory(version, data)
        with (args.output / "inventory.json").open("x", encoding="utf-8") as stream:
            json.dump(result, stream, ensure_ascii=False, indent=2)
        print("Both startup inventories saved")
        return
    result = {}
    for version in INPUTS:
        evidence, data, instructions = analyze(version)
        result[version] = evidence
        selected = []
        seeds = [0xa0000000]
        for rows in evidence["candidate_xrefs"].values():
            for row in rows:
                if "pc" in row:
                    pc = int(row["pc"], 16)
                    seeds.append(pc)
        for start in sorted(set(seeds)):
            selected.append(f"\n--- candidate context {hex(start)}; bounds do not prove a function ---")
            first = bisect_left(instructions, (start - 80,))
            last = bisect_left(instructions, (start + 112,))
            for pc, size, mnemonic, operands in instructions[first:last]:
                selected.append(f"{pc:08x} {hexbytes(data, pc, size):12} {mnemonic:24} {operands}")
        target = args.output / (version + "-candidate-disassembly.txt")
        if target.exists():
            parser.error("output already exists")
        target.write_text("\n".join(selected), encoding="utf-8", newline="\n")
    target = args.output / "comparison.json"
    with target.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps({v: {k: result[v][k] for k in ("version", "sha256", "size", "startup_gp", "scan")} for v in result}, indent=2))


if __name__ == "__main__":
    main()
