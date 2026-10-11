"""Exercise the Python test bridge's canonical encoder without starting services."""
import ast
import json
from pathlib import Path
import zlib

root = Path(__file__).resolve().parents[1]
module = ast.parse((root / "tools/test-bridge/bridge.py").read_text(encoding="utf-8"))
encoder = next(n for n in module.body if isinstance(n, ast.FunctionDef) and n.name == "tpl_canon")
scope = {"json": json}
exec(compile(ast.Module(body=[encoder], type_ignores=[]), "bridge.py:tpl_canon", "exec"), scope)
for path, expected in [
    ("tools/test-bridge/templates/quad.json", "666c6404"),
    ("bridge/crates/core/tests/fixtures/codex-status-a-400x300.json", "ffa2bdbd"),
]:
    template = json.loads((root / path).read_text(encoding="utf-8"))
    raw = scope["tpl_canon"](template)
    assert f"{zlib.crc32(raw) & 0xffffffff:08x}" == expected
    assert json.loads(raw) == template
    print(f"PASS: Python canonical {path}: {expected}")
