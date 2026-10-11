"""Generator contract check; --fixture writes a tiny deterministic CSFN v2."""
import importlib.util
import pathlib
import struct
import sys
import types

# Container packing needs no FreeType installation or font download.
sys.modules.setdefault("freetype", types.SimpleNamespace(
    FT_LOAD_FORCE_AUTOHINT=1, FT_LOAD_NO_HINTING=2))
root = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("rasterize", root / "tools/note4-fonts/rasterize_ttf.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
face = types.SimpleNamespace(hint="native", line_height=8, base_line=1)
cps = module.ASCII + [0x4E2D, 0x6587, 0x20000]
glyphs, blob = bytearray(), bytearray()
for cp in cps:
    rows = b"\x80\x40" if cp < 128 else b"\xe0\xa0\xe0"
    glyphs += struct.pack("<HHBBbb", len(blob), 4 * 16 if cp < 128 else 8 * 16,
                          2 if cp < 128 else 3, len(rows), 0, 0)
    blob += rows
packed = module.pack_container("ntthin18", "fixture.ttf", "unicode", face, 8,
                               blob, glyphs, len(cps), cps)
assert struct.unpack_from("<H", packed, 4)[0] == 2
assert struct.unpack_from("<I", packed, 20)[0] == 98
assert struct.unpack_from("<I", packed, 12)[0] == module.crc32(packed[64:])
for bad in [cps[:-1] + [0xD800], cps[:-1] + [0x110000], cps[:-1] + [0x6587]]:
    try:
        module.pack_container("ntthin18", "fixture.ttf", "unicode", face, 8,
                              blob, glyphs, len(cps), bad)
    except ValueError:
        pass
    else:
        raise AssertionError("bad codepoint accepted")
if len(sys.argv) == 3 and sys.argv[1] == "--fixture":
    pathlib.Path(sys.argv[2]).write_bytes(packed)
print(f"CSFN v2 generator: {len(cps)} glyphs, {len(packed)} bytes; invalid tables rejected")
