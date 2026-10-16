"""Read-only exact-0031 OTA/UI evidence; no network, patching or device access."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import inspect_ble as baseline


# Research windows, not asserted function extents or callable ABI definitions.
WINDOWS = {
    "ota_app_reference": (0xA00622A4, 0xA00622FA),
    "signed_file_parser": (0xA006851E, 0xA00687AE),
    "native_ota_open": (0xA009B740, 0xA009B824),
    "native_ota_io": (0xA009B824, 0xA009B8DA),
    "native_ota_crc": (0xA009B8DA, 0xA009BA36),
    "stock_popup_create": (0xA00BBC00, 0xA00BBD36),
    "stock_popup_timer": (0xA00F3B44, 0xA00F3BD4),
}
TERMS = (
    "/dev/ota", "ota_app", "miio_ota_verify", "arch_ota",
    "lv_page_ui_app_ota_pop_up_create", "lv_page_ui_setting_dtl6_ota_create",
    "j_ui_app_ota_update_timer_xcb", "j_ui_setting_dtl6_ota_statu_change_cb",
)


def inspect(data):
    baseline.validate(data)
    analysis = baseline.analysis
    strings = {}
    for term in TERMS:
        offsets = analysis.find_all(data, term.encode("ascii") + b"\0")
        for offset in offsets:
            strings[offset + analysis.DELTA] = term
    instructions = analysis.disassembler().disasm_lite(data[0x1000:0x1B0000], 0xA0000000)
    references = [
        {"pc": hex(pc), "target": hex(target), "string": strings[target]}
        for pc, target in baseline.address_references(instructions) if target in strings
    ]
    windows = {}
    for name, (start, end) in WINDOWS.items():
        raw = data[start - analysis.DELTA:end - analysis.DELTA]
        windows[name] = {
            "start_va": hex(start), "end_va_exclusive": hex(end),
            "sha256": hashlib.sha256(raw).hexdigest(), "bytes_hex": raw.hex(),
            "instructions": [
                {"pc": hex(pc), "bytes": data[pc-analysis.DELTA:pc-analysis.DELTA+size].hex(),
                 "mnemonic": mnemonic, "operands": operands}
                for pc, size, mnemonic, operands in analysis.decode(raw, start)
            ],
        }
    return {"image_sha256": baseline.SHA256, "image_bytes": len(data),
            "strings": {hex(address): term for address, term in strings.items()},
            "candidate_references": references, "windows": windows,
            "limitations": "Selected original bytes and linear basic-block references only; "
            "not complete function bounds, callable ABI, task isolation, boot recovery or hardware proof. "
            "VA=file offset+0x9ffff000 is not a physical flash write address."}


def self_test(data, result):
    # Assert observed instructions rather than substituting an imagined OTA service.
    instructions = {int(i["pc"], 16): i for window in result["windows"].values()
                    for i in window["instructions"]}
    timer_call = instructions[0xA00BBD0C]
    assert timer_call["mnemonic"] == "jal"
    assert 0xA00BBD0C + int(timer_call["operands"], 0) == 0xA007D500
    assert any(r["pc"] == "0xa00f3b64" and
               r["string"] == "j_ui_app_ota_update_timer_xcb"
               for r in result["candidate_references"])
    assert any(r["pc"] == "0xa00bbc1c" and
               r["string"] == "lv_page_ui_app_ota_pop_up_create"
               for r in result["candidate_references"])
    changed = bytearray(data)
    changed[0xA00BBD0C - baseline.analysis.DELTA] ^= 1
    for invalid in (data[:-1], bytes(changed)):
        try:
            baseline.validate(invalid)
        except ValueError:
            pass
        else:
            raise AssertionError("changed/truncated original accepted")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if not args.output.parent.is_dir() or args.output.exists():
        parser.error("output parent must exist and output must be new")
    data = args.image.read_bytes()
    result = inspect(data)
    if args.self_test:
        self_test(data, result)
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps({"sha256": result["image_sha256"], "windows": len(WINDOWS),
                      "references": len(result["candidate_references"]),
                      "self_test": args.self_test, "output": str(args.output)}))


if __name__ == "__main__":
    main()
