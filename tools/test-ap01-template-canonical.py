"""Cross-check the Python canonical writer against shared C++/Rust evidence."""
import ast
import json
from pathlib import Path
import zlib

root = Path(__file__).resolve().parents[1]
source = root / 'tools/test-bridge/bridge.py'
node = next(node for node in ast.parse(source.read_text(encoding='utf-8')).body
            if isinstance(node, ast.FunctionDef) and node.name == 'tpl_canon')
scope = {'json':json}
exec(compile(ast.Module(body=[node], type_ignores=[]),str(source),'exec'),scope)
template = json.loads((root/'tools/test-bridge/templates/codex-status-ap01.json').read_text(encoding='utf-8'))
data = scope['tpl_canon'](template)
assert '周额度'.encode() in data and b'\\u5468' not in data
manifest = json.loads((root/'artifacts/ap01-template-integration/template-manifest.json').read_text())
crc = f'{zlib.crc32(data):08x}'
assert crc == manifest['source_crc'], (crc, manifest['source_crc'])
print(f'Python / Rust / shared C++ canonical CRC: {crc}')
