"""Stdlib regression checks for the bounded 0041 unsigned-OTA patch."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True

import patch_0041_unsigned_ota as patch


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "artifacts/ap01-research/ap01-1.0.2_0041.bin"
CANDIDATE = ROOT / "artifacts/ap01-research/rehost-20261016/ap01-1.0.2_0041-unsigned-ota.bin"
MANIFEST = CANDIDATE.with_name(CANDIDATE.stem + ".manifest.json")


class UnsignedOtaPatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = SOURCE.read_bytes()
        cls.candidate, cls.validation = patch.patch_bytes(cls.original)

    def test_c_j_encoding_and_target(self):
        self.assertEqual(patch.c_j_encode(patch.PATCH_VA, patch.PATCH_TARGET_VA), patch.NEW_BYTES)
        self.assertEqual(patch.c_j_decode(patch.PATCH_VA, patch.NEW_BYTES), patch.PATCH_TARGET_VA)

    def test_signed_file_semantics_preserve_flag(self):
        self.assertEqual(patch.signed_file_flow(False, patched=False), ("unsigned_reject",))
        self.assertEqual(patch.signed_file_flow(False, patched=True),
                         ("continue", "skip_signature_validation"))
        self.assertEqual(patch.signed_file_flow(True, patched=False),
                         ("continue", "validate_signature"))

    def test_real_rv32_load_and_branch_with_both_flag_values(self):
        # Independent instruction execution, rather than testing only the
        # Python encoder/decoder against each other. Reuse local native deps.
        deps = ROOT / "artifacts/ap01-research/app0031-20261013/deps"
        if not (deps / "unicorn").is_dir():
            self.skipTest("local Unicorn dependency unavailable")
        sys.path.insert(0, str(deps))
        import unicorn as uc
        from unicorn.riscv_const import UC_RISCV_REG_PC, UC_RISCV_REG_S2, UC_RISCV_REG_A5
        for image, patched in [(self.original, False), (self.candidate, True)]:
            for flag in (0, 1):
                with self.subTest(patched=patched, flag=flag):
                    machine = uc.Uc(uc.UC_ARCH_RISCV, uc.UC_MODE_RISCV32)
                    machine.mem_map(0xA006F000, 4096)
                    offset = 0xA006F000 - patch.MAP_BASE
                    machine.mem_write(0xA006F000, image[offset:offset + 4096])
                    machine.mem_map(0x62FC0000, 4096)
                    machine.mem_write(0x62FC000C, bytes([flag]))
                    machine.reg_write(UC_RISCV_REG_S2, 0x62FC0000)
                    machine.emu_start(0xA006F7F4, 0xA006F900, count=3)
                    expected = patch.PATCH_TARGET_VA if patched or flag else patch.PATCH_VA + 2
                    self.assertEqual(machine.reg_read(UC_RISCV_REG_PC), expected)
                    self.assertEqual(machine.reg_read(UC_RISCV_REG_A5), flag)
                    self.assertEqual(bytes(machine.mem_read(0x62FC000C, 1)), bytes([flag]))

    def test_source_identity_and_patch_ranges(self):
        self.assertEqual(len(self.original), patch.IMAGE_SIZE)
        self.assertEqual(patch.sha256(self.original), patch.IMAGE_SHA256)
        self.assertEqual(self.original[patch.PATCH_OFFSET:patch.PATCH_OFFSET + 2], patch.OLD_BYTES)
        self.assertEqual(self.validation["changed_ranges"], [
            {
                "start": patch.PATCH_OFFSET,
                "end_exclusive": patch.PATCH_OFFSET + 2,
                "length": 2,
                "before": patch.OLD_BYTES.hex(),
                "after": patch.NEW_BYTES.hex(),
            },
            {
                "start": len(self.original) - 4,
                "end_exclusive": len(self.original),
                "length": 4,
                "before": self.original[-4:].hex(),
                "after": self.candidate[-4:].hex(),
            },
        ])
        for index, (old, new) in enumerate(zip(self.original, self.candidate)):
            if old != new:
                self.assertTrue(index in range(patch.PATCH_OFFSET, patch.PATCH_OFFSET + 2)
                                or index >= len(self.original) - 4)

    def test_header_and_recovery_crc_readback(self):
        source = patch.image_metadata(self.original, require_identity=True)
        output = patch.image_metadata(self.candidate, require_identity=False)
        self.assertEqual(source["header_crc"], output["header_crc"])
        self.assertEqual(output["recovery"]["stored_crc"], patch.recovery_crc(self.candidate))
        self.assertEqual(output["recovery"]["stored_length"], patch.IMAGE_SIZE)

    def test_corrupt_input_is_rejected(self):
        corrupt = bytearray(self.original)
        corrupt[patch.PATCH_OFFSET] ^= 1
        with self.assertRaises(patch.PatchError):
            patch.patch_bytes(bytes(corrupt))

    def test_existing_output_is_rejected_without_write(self):
        with self.assertRaises((FileExistsError, patch.PatchError)):
            patch.patch_file(SOURCE, SOURCE)

    def test_generated_candidate_and_manifest_when_present(self):
        if not CANDIDATE.exists():
            self.skipTest("candidate is generated by the assigned CLI validation step")
        self.assertTrue(MANIFEST.is_file())
        disk = CANDIDATE.read_bytes()
        self.assertEqual(disk, self.candidate)
        document = json.loads(MANIFEST.read_text(encoding="utf-8"))
        self.assertEqual(document["input"]["sha256"], patch.IMAGE_SHA256)
        self.assertEqual(document["output"]["sha256"], patch.sha256(disk))
        self.assertEqual(document["patch"]["old_bytes"], patch.OLD_BYTES.hex())
        self.assertEqual(document["patch"]["new_bytes"], patch.NEW_BYTES.hex())
        self.assertTrue(document["header_crc_unchanged"])


if __name__ == "__main__":
    unittest.main()
