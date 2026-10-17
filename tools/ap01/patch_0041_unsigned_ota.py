"""Patch the AP01 1.0.2_0041 unsigned-app OTA rejection gate offline.

The input is a fixed, verified 0041 image.  The patch changes one compressed
conditional branch to a compressed unconditional jump and refreshes only the
image's trailing recovery CRC.  It does not create directories, overwrite
files, flash hardware, or contact a device.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
import zlib

sys.dont_write_bytecode = True

IMAGE_SIZE = 6_769_704
IMAGE_SHA256 = "972db4c136c7ed9e24a83c07c1a7fd62040ca018b08ca285216d26b1fee3c6b9"
MAP_BASE = 0x9FFFF000
PATCH_VA = 0xA006F7FC
PATCH_OFFSET = PATCH_VA - MAP_BASE
PATCH_TARGET_VA = 0xA006F8AA
OLD_BYTES = bytes.fromhex("dde7")
NEW_BYTES = bytes.fromhex("7da0")
RECOVERY_SIZE = 40
RECOVERY_TAG = b"0x5245434f56455259544147"
HEADER_CRC_OFFSET = 0xFC
RECOVERY_CRC_OFFSET_FROM_END = 4


class PatchError(ValueError):
    """Raised when an input or candidate violates a patch guard."""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def c_j_encode(source: int, target: int) -> bytes:
    """Encode a RISC-V compressed C.J relative jump."""
    delta = target - source
    if delta & 1 or not -(1 << 11) <= delta < (1 << 11):
        raise PatchError("C.J target is outside its signed 12-bit halfword range")
    immediate = delta & 0xFFF
    instruction = (
        (0b101 << 13)
        | (((immediate >> 11) & 1) << 12)
        | (((immediate >> 4) & 1) << 11)
        | (((immediate >> 8) & 3) << 9)
        | (((immediate >> 10) & 1) << 8)
        | (((immediate >> 6) & 1) << 7)
        | (((immediate >> 7) & 1) << 6)
        | (((immediate >> 1) & 7) << 3)
        | (((immediate >> 5) & 1) << 2)
        | 0b01
    )
    return struct.pack("<H", instruction)


def c_j_decode(pc: int, encoded: bytes) -> int:
    """Decode a RISC-V compressed C.J and return its absolute target."""
    if len(encoded) != 2:
        raise PatchError("C.J encoding must be exactly two bytes")
    instruction = struct.unpack("<H", encoded)[0]
    if instruction & 0x3 != 0x1 or (instruction >> 13) & 0x7 != 0x5:
        raise PatchError("encoding is not C.J")
    immediate = (
        (((instruction >> 12) & 1) << 11)
        | (((instruction >> 11) & 1) << 4)
        | (((instruction >> 9) & 3) << 8)
        | (((instruction >> 8) & 1) << 10)
        | (((instruction >> 7) & 1) << 6)
        | (((instruction >> 6) & 1) << 7)
        | (((instruction >> 3) & 7) << 1)
        | (((instruction >> 2) & 1) << 5)
    )
    if immediate & 0x800:
        immediate -= 0x1000
    return (pc + immediate) & 0xFFFFFFFF


def recovery_crc(data: bytes) -> int:
    """Calculate the CRC stored in the final four bytes of the image."""
    if len(data) < RECOVERY_SIZE:
        raise PatchError("image is shorter than its recovery record")
    return (zlib.crc32(data[:-RECOVERY_CRC_OFFSET_FROM_END], 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF


def recovery_metadata(data: bytes) -> dict[str, int | str]:
    if len(data) < RECOVERY_SIZE:
        raise PatchError("image is shorter than its recovery record")
    offset = len(data) - RECOVERY_SIZE
    if data[offset:offset + len(RECOVERY_TAG)] != RECOVERY_TAG:
        raise PatchError("recovery tag mismatch")
    stored_length = struct.unpack_from(">I", data, offset + 32)[0]
    stored_crc = struct.unpack_from("<I", data, offset + 36)[0]
    calculated_crc = recovery_crc(data)
    if stored_length != len(data):
        raise PatchError("recovery length mismatch")
    if stored_crc != calculated_crc:
        raise PatchError("recovery CRC mismatch")
    return {
        "offset": offset,
        "size": RECOVERY_SIZE,
        "stored_length": stored_length,
        "stored_crc": stored_crc,
        "calculated_crc": calculated_crc,
    }


def image_metadata(data: bytes, *, require_identity: bool) -> dict[str, object]:
    if len(data) != IMAGE_SIZE:
        raise PatchError(f"image size mismatch: expected {IMAGE_SIZE}, got {len(data)}")
    digest = sha256(data)
    if require_identity and digest != IMAGE_SHA256:
        raise PatchError("input SHA256 identity mismatch")
    if data[:4] != b"BFNP" or data[8:12] != b"FCFG" or data[0x64:0x68] != b"PCFG":
        raise PatchError("image header markers mismatch")
    header_stored = struct.unpack_from("<I", data, HEADER_CRC_OFFSET)[0]
    header_calculated = zlib.crc32(data[:HEADER_CRC_OFFSET]) & 0xFFFFFFFF
    if header_stored != header_calculated:
        raise PatchError("image header CRC mismatch")
    recovery = recovery_metadata(data)
    return {
        "bytes": len(data),
        "sha256": digest,
        "header_crc": header_stored,
        "recovery": recovery,
    }


def signed_file_flow(signed_file: bool, *, patched: bool) -> tuple[str, ...]:
    """Model the two existing conditionals around the patched branch."""
    if not signed_file and not patched:
        return ("unsigned_reject",)
    return ("continue", "validate_signature" if signed_file else "skip_signature_validation")


def changed_ranges(before: bytes, after: bytes) -> list[dict[str, object]]:
    if len(before) != len(after):
        raise PatchError("candidate size changed")
    ranges: list[dict[str, object]] = []
    start = None
    for index, (old, new) in enumerate(zip(before, after)):
        if old != new and start is None:
            start = index
        if start is not None and (old == new or index + 1 == len(before)):
            end = index if old == new else index + 1
            ranges.append({
                "start": start,
                "end_exclusive": end,
                "length": end - start,
                "before": before[start:end].hex(),
                "after": after[start:end].hex(),
            })
            start = None
    return ranges


def patch_bytes(original: bytes) -> tuple[bytes, dict[str, object]]:
    """Return a guarded candidate and its machine-readable validation data."""
    source = image_metadata(original, require_identity=True)
    if original[PATCH_OFFSET:PATCH_OFFSET + len(OLD_BYTES)] != OLD_BYTES:
        raise PatchError("unsigned OTA branch bytes mismatch")
    if c_j_encode(PATCH_VA, PATCH_TARGET_VA) != NEW_BYTES:
        raise PatchError("internal C.J encoding guard mismatch")
    if c_j_decode(PATCH_VA, NEW_BYTES) != PATCH_TARGET_VA:
        raise PatchError("internal C.J decode guard mismatch")

    candidate = bytearray(original)
    candidate[PATCH_OFFSET:PATCH_OFFSET + len(NEW_BYTES)] = NEW_BYTES
    old_recovery_crc = struct.unpack_from("<I", candidate, len(candidate) - 4)[0]
    new_recovery_crc = recovery_crc(bytes(candidate))
    struct.pack_into("<I", candidate, len(candidate) - 4, new_recovery_crc)
    candidate_bytes = bytes(candidate)

    if candidate_bytes[:HEADER_CRC_OFFSET + 4] != original[:HEADER_CRC_OFFSET + 4]:
        raise PatchError("header CRC region changed")
    candidate_recovery = recovery_metadata(candidate_bytes)
    ranges = changed_ranges(original, candidate_bytes)
    expected_ranges = {
        (PATCH_OFFSET, PATCH_OFFSET + len(NEW_BYTES)),
        (len(candidate) - 4, len(candidate)),
    }
    actual_ranges = {(item["start"], item["end_exclusive"]) for item in ranges}
    if actual_ranges != expected_ranges:
        raise PatchError(f"unexpected changed ranges: {actual_ranges}")
    if any(
        original[index] != candidate_bytes[index]
        and not (PATCH_OFFSET <= index < PATCH_OFFSET + 2 or index >= len(candidate) - 4)
        for index in range(len(candidate))
    ):
        raise PatchError("candidate changed bytes outside the branch and recovery CRC")
    candidate_info = image_metadata(candidate_bytes, require_identity=False)
    validation = {
        "input": source,
        "output": candidate_info,
        "changed_ranges": ranges,
        "header_crc_unchanged": source["header_crc"] == candidate_info["header_crc"],
        "recovery_crc": {
            "old": old_recovery_crc,
            "new": new_recovery_crc,
            "readback": candidate_recovery["stored_crc"],
            "algorithm": "crc32(data[:-4], initial=0xffffffff) xor 0xffffffff",
        },
        "outside_changed_ranges_unchanged": True,
    }
    return candidate_bytes, validation


def manifest_path(output: Path) -> Path:
    return output.with_name(output.stem + ".manifest.json")


def patch_file(input_path: Path, output_path: Path) -> tuple[Path, dict[str, object]]:
    input_path = input_path.resolve()
    output_path = output_path.resolve()
    manifest = manifest_path(output_path)
    if not input_path.is_file():
        raise PatchError(f"input is not an existing file: {input_path}")
    if not output_path.parent.is_dir():
        raise PatchError(f"output directory must already exist: {output_path.parent}")
    if output_path.exists() or manifest.exists():
        raise FileExistsError("output or manifest already exists; refusing overwrite")
    original = input_path.read_bytes()
    candidate, validation = patch_bytes(original)
    document = {
        "tool": "tools/ap01/patch_0041_unsigned_ota.py",
        "target": "AP01 1.0.2_0041 unsigned app OTA rejection gate",
        "flashed": False,
        "input": {"path": str(input_path), **validation["input"]},
        "output": {"path": str(output_path), **validation["output"]},
        "patch": {
            "file_offset": PATCH_OFFSET,
            "file_offset_hex": hex(PATCH_OFFSET),
            "va": hex(PATCH_VA),
            "old_bytes": OLD_BYTES.hex(),
            "new_bytes": NEW_BYTES.hex(),
            "instruction": "c.j",
            "target_va": hex(PATCH_TARGET_VA),
            "target_delta": PATCH_TARGET_VA - PATCH_VA,
            "signed_file_register": "a5",
            "behavior": "bypass unsigned rejection gate; preserve signed_file=false for the downstream signature conditional",
        },
        "recovery": validation["recovery_crc"],
        "header_crc_unchanged": validation["header_crc_unchanged"],
        "changed_ranges": validation["changed_ranges"],
        "validation": {
            "outside_changed_ranges_unchanged": validation["outside_changed_ranges_unchanged"],
            "candidate_recovery_crc_readback": True,
            "c_j_target_decodes": c_j_decode(PATCH_VA, NEW_BYTES) == PATCH_TARGET_VA,
        },
        "limitations": [
            "Offline candidate only; no flash, device, network, Bridge, bootloader, or stock-0041 acceptance was run.",
            "The patch changes this running image's future OTA path and does not provide a first-install route.",
            "Existing certificate, package signature, CRC, version, and model checks outside this gate were not removed.",
        ],
    }
    with output_path.open("xb") as handle:
        handle.write(candidate)
    with manifest.open("x", encoding="utf-8", newline="\n") as handle:
        json.dump(document, handle, ensure_ascii=False, indent=2)
        handle.write("\n")
    return output_path, document


def self_test() -> None:
    assert c_j_encode(PATCH_VA, PATCH_TARGET_VA) == NEW_BYTES
    assert c_j_decode(PATCH_VA, NEW_BYTES) == PATCH_TARGET_VA
    assert signed_file_flow(False, patched=False) == ("unsigned_reject",)
    assert signed_file_flow(False, patched=True) == ("continue", "skip_signature_validation")
    assert signed_file_flow(True, patched=False) == ("continue", "validate_signature")
    source_path = Path(__file__).resolve().parents[2] / "artifacts/ap01-research/ap01-1.0.2_0041.bin"
    original = source_path.read_bytes()
    source = image_metadata(original, require_identity=True)
    candidate, validation = patch_bytes(original)
    assert source["header_crc"] == validation["output"]["header_crc"]
    assert recovery_metadata(candidate)["stored_crc"] == recovery_crc(candidate)
    corrupt = bytearray(original)
    corrupt[PATCH_OFFSET] ^= 1
    try:
        patch_bytes(bytes(corrupt))
    except PatchError:
        pass
    else:
        raise AssertionError("corrupt input was accepted")
    try:
        patch_file(source_path, source_path)
    except (FileExistsError, PatchError):
        pass
    else:
        raise AssertionError("existing output was not rejected")
    print("self-test: C.J encode/decode, signed_file flow, source guards, CRC readback, corruption rejection, no-overwrite PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.input is None or args.output is None:
        parser.error("--input and --output are required unless --self-test is used")
    output, document = patch_file(args.input, args.output)
    print(json.dumps({"output": str(output), "bytes": document["output"]["bytes"], "sha256": document["output"]["sha256"], "manifest": str(manifest_path(output))}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
