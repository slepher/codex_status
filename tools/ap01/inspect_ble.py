"""Read-only 0031 BLE ABI investigation; produces no firmware or device writes."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import struct
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("ap01_analysis", Path(__file__).with_name("__main__.py"))
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)

SHA256 = "8a721fc8ef25458d415b2460e4a251e0503a82f7743fdff85b12612190e5c1cb"
TERMS = re.compile(r"gatt|bond|smp_|security_changed|bt_conn|auth_cb|pairing|mible_gap|ble_stack", re.I)


def validate(data):
    if len(data) != 6804520 or hashlib.sha256(data).hexdigest() != SHA256:
        raise ValueError("exact original 0031 image required")
    analysis.validate_baseline(data)


def address_references(instructions):
    """Conservative constants inside one linear basic block; no ABI inference."""
    registers = {}
    for pc, size, mnemonic, operands in instructions:
        fields = [part.strip() for part in operands.split(",")]
        if mnemonic in ("lui", "c.lui", "auipc"):
            registers[fields[0]] = ((int(fields[1], 0) << 12) +
                                     (pc if mnemonic == "auipc" else 0)) & 0xffffffff
            continue
        if mnemonic in ("addi", "c.addi"):
            source = fields[1] if mnemonic == "addi" else fields[0]
            value = registers.get(source)
            registers.pop(fields[0], None)
            if value is not None:
                target = (value + int(fields[-1], 0)) & 0xffffffff
                registers[fields[0]] = target
                yield pc, target
            continue
        if mnemonic in ("mv", "c.mv"):
            value = registers.get(fields[1])
            registers.pop(fields[0], None)
            if value is not None:
                registers[fields[0]] = value
            continue
        if (mnemonic.startswith(("b", "j", "c.b", "c.j")) or
                mnemonic in ("ret", "undecoded_instruction", "ecall", "ebreak", "c.ebreak")):
            registers.clear()
        elif mnemonic not in ("sw", "sh", "sb", "c.sw", "c.swsp", "fence", "nop", "c.nop"):
            registers.pop(fields[0], None)


def inspect(data):
    validate(data)
    strings = {
        match.start() + analysis.DELTA: match.group()[:-1].decode("ascii")
        for match in re.finditer(rb"[ -~]{5,}\x00", data)
        if TERMS.search(match.group()[:-1].decode("ascii"))
    }
    references = []
    # Same bounded code sweep as the existing 0031 tool, not a complete CFG.
    instructions = list(analysis.disassembler().disasm_lite(data[0x1000:0x1B0000], 0xA0000000))
    for pc, target in address_references(instructions):
        if target in strings:
            references.append({"pc": hex(pc), "target": hex(target),
                               "kind": "basic_block_constant", "string": strings[target]})
    pointers = []
    for address, value in strings.items():
        for offset in analysis.find_all(data, struct.pack("<I", address)):
            pointers.append({"offset": hex(offset), "target": hex(address), "string": value})
    return {"sha256": SHA256, "bytes": len(data), "capstone": analysis.capstone.__version__,
            "strings": {hex(k): v for k, v in strings.items()},
            "candidate_references": references, "raw_pointer_matches": pointers,
            "unknown_instructions": sum(i[2] == "undecoded_instruction" for i in instructions),
            "limitations": "Linear basic-block constants only; byte matches are not proven tables. "
                           "Function boundaries, ABI, security and lifetime require further verification. "
                           "VA=file offset+0x9ffff000 is not a physical flash write address."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path, help="new JSON file in an existing directory")
    parser.add_argument("--window", action="append", default=[], help="explicit start:end virtual addresses")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if not args.output.parent.is_dir() or args.output.exists():
        parser.error("output parent must exist and output must be new")
    data = args.image.read_bytes()
    validate(data)
    if args.self_test:
        prefix = [(0, 4, "lui", "a0, 0xa0533")]
        suffix = [(8, 4, "addi", "a1, a0, -0x100")]
        assert list(address_references(prefix + [(4, 4, "li", "a2, 1")] + suffix)) == [(8, 0xa0532f00)]
        for mnemonic, operands in (("lw", "a0, 0(a2)"), ("jal", "0x100"),
                                    ("bne", "a1, a2, 0x100"), ("undecoded_instruction", "0x0b")):
            assert not list(address_references(prefix + [(4, 4, mnemonic, operands)] + suffix))
        changed = bytearray(data)
        changed[0x1000] ^= 1
        for invalid in (data[:-1], bytes(changed)):
            try:
                validate(invalid)
            except ValueError:
                pass
            else:
                raise AssertionError("invalid baseline accepted")
        analysis.self_test(data)
    result = inspect(data)
    windows = {}
    for window in args.window:
        start, end = (int(part, 0) for part in window.split(":"))
        if not 0xA0000000 <= start < end <= 0xA01AF000 or start % 2 or end % 2:
            parser.error("window must be halfword-aligned inside the known code region")
        windows[window] = [f"{pc:08x} {data[pc-analysis.DELTA:pc-analysis.DELTA+size].hex():12s} {mnemonic:22s} {operands}"
                           for pc, size, mnemonic, operands in analysis.decode(
                               data[start-analysis.DELTA:end-analysis.DELTA], start)]
    result["explicit_windows"] = windows
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps({"sha256": SHA256, "strings": len(result["strings"]),
                      "references": len(result["candidate_references"]), "output": str(args.output)}))


if __name__ == "__main__":
    main()
