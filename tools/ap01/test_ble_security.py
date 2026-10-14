"""Execute the original 0031 ATT permission checker; never patch a ROM or device."""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct
import sys

sys.dont_write_bytecode = True
from inspect_ble import SHA256, validate
import unicorn
from unicorn.riscv_const import (UC_RISCV_REG_A0, UC_RISCV_REG_A1,
                                UC_RISCV_REG_A2, UC_RISCV_REG_PC,
                                UC_RISCV_REG_RA, UC_RISCV_REG_S0,
                                UC_RISCV_REG_SP)

START, END = 0xA01A99EC, 0xA01A9A34
CODE_SHA256 = "1b4629d8e55c152d4bb86ee4e9bc2476c75cddbaf846bff8c00e0c4984e2e0ea"
ATTR, STACK, STOP = 0x20000000, 0x20002000, 0x20003000


def expected(perm, mask, read, write):
    # SDK permission semantics with the no-SMP branch. This hypothesis is
    # checked against execution of the *original machine code*, not a port.
    if mask & 1 and (not perm & 0x15 or not read):
        return 2
    if mask & 2 and (not perm & 0x2A or not write):
        return 3
    if perm & mask & 0x30:
        return 5
    if perm & mask & 0x0C:
        return 15
    return 0


def check(image):
    validate(image)
    code = image[START - 0x9FFFF000:END - 0x9FFFF000]
    if hashlib.sha256(code).hexdigest() != CODE_SHA256:
        raise ValueError("permission-checker bytes differ")
    uc = unicorn.Uc(unicorn.UC_ARCH_RISCV, unicorn.UC_MODE_RISCV32)
    uc.mem_map(START & ~0xFFF, 0x1000, unicorn.UC_PROT_ALL)
    uc.mem_write(START, code)
    uc.mem_protect(START & ~0xFFF, 0x1000, unicorn.UC_PROT_READ | unicorn.UC_PROT_EXEC)
    uc.mem_map(ATTR, 0x1000, unicorn.UC_PROT_READ | unicorn.UC_PROT_WRITE)
    uc.mem_map(STACK - 0x1000, 0x1000, unicorn.UC_PROT_READ | unicorn.UC_PROT_WRITE)
    uc.mem_map(STOP, 0x1000, unicorn.UC_PROT_READ | unicorn.UC_PROT_EXEC)

    def run(perm, mask, read, write, unused_argument=0):
        # Function-pointer values are nonzero sentinels in unmapped memory.
        # Only presence is relevant; calling either must fault the test.
        attr = struct.pack("<IIIIHBB", 0, 0xDEAD0000 if read else 0,
                           0xDEAD1000 if write else 0, 0, 1, perm, 0)
        uc.mem_write(ATTR, attr)
        for reg, value in ((UC_RISCV_REG_A0, ATTR), (UC_RISCV_REG_A1, mask),
                           (UC_RISCV_REG_A2, unused_argument),
                           (UC_RISCV_REG_SP, STACK), (UC_RISCV_REG_RA, STOP),
                           (UC_RISCV_REG_S0, 0x12345678)):
            uc.reg_write(reg, value)
        uc.emu_start(START, STOP, timeout=100_000, count=64)
        if (uc.reg_read(UC_RISCV_REG_PC) != STOP or
                uc.reg_read(UC_RISCV_REG_SP) != STACK or
                uc.reg_read(UC_RISCV_REG_S0) != 0x12345678):
            raise AssertionError("checker did not return with preserved stack/frame")
        if bytes(uc.mem_read(ATTR, len(attr))) != attr:
            raise AssertionError("checker mutated the attribute")
        return uc.reg_read(UC_RISCV_REG_A0)

    named = {}
    for name, perm, mask, read, write, result in (
            ("plain_read", 1, 0x15, True, False, 0),
            ("plain_write", 2, 0x2A, False, True, 0),
            ("missing_read_callback", 1, 0x15, False, False, 2),
            ("missing_write_callback", 2, 0x2A, False, False, 3),
            ("encrypted_read", 4, 0x15, True, False, 15),
            ("encrypted_write", 8, 0x2A, False, True, 15),
            ("authenticated_read", 0x10, 0x15, True, False, 5),
            ("authenticated_write", 0x20, 0x2A, False, True, 5),
            ("notification_encryption", 4, 0x14, True, True, 15),
            ("notification_authentication", 0x10, 0x14, True, True, 5)):
        actual = run(perm, mask, read, write)
        if actual != result:
            raise AssertionError((name, result, actual))
        named[name] = actual

    counts = Counter()
    cases = 0
    # Every permission byte, all callback-presence combinations, all three
    # observed caller masks. A2 is deliberately an unmapped address on pass 2:
    # this optimized checker does not receive/read a connection context.
    for perm in range(256):
        for mask in (0x15, 0x2A, 0x14):
            for read, write in ((False, False), (False, True),
                                (True, False), (True, True)):
                for unused in (0, 0xDEAD2000):
                    actual = run(perm, mask, read, write, unused)
                    want = expected(perm, mask, read, write)
                    if actual != want:
                        raise AssertionError((perm, mask, read, write, want, actual))
                    counts[str(actual)] += 1
                    cases += 1
    return {"image_sha256": SHA256, "code_sha256": CODE_SHA256,
            "function_va": hex(START), "function_bytes": len(code),
            "unicorn": unicorn.__version__, "named_cases": named,
            "matrix_cases": cases, "return_counts": dict(counts),
            "result": "original_checker_unconditionally_rejects_security_permissions",
            "limitations": "Exact original 0031 checker only. Does not execute a radio, "
                           "prove all SMP code absent, or validate a replacement Bluetooth stack."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists() or not args.output.parent.is_dir():
        parser.error("output must be new, in an existing directory")
    result = check(args.image.read_bytes())
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    print(json.dumps({"result": result["result"], "matrix_cases": result["matrix_cases"],
                      "output": str(args.output)}))


if __name__ == "__main__":
    main()
