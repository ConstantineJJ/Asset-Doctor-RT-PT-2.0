"""Project-authored source UV channels; numeric data only, no external assets."""
from pathlib import Path
import hashlib
import json
import struct

root = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures'
binary = bytearray()
views, accessors, meshes = [], [], []
bad_offset = None


def attribute(values, kind):
    binary.extend(b'\0' * (-len(binary) % 4))
    offset = len(binary)
    flat = [v for value in values for v in value]
    binary.extend(struct.pack('<' + 'f' * len(flat), *flat))
    views.append({'buffer': 0, 'byteOffset': offset, 'byteLength': len(flat) * 4})
    entry = {'bufferView': len(views) - 1, 'componentType': 5126, 'count': len(values), 'type': kind}
    if kind == 'VEC3':
        entry.update(min=[min(v[i] for v in values) for i in range(3)], max=[max(v[i] for v in values) for i in range(3)])
    accessors.append(entry)
    return len(accessors) - 1, offset


for number in range(3):
    points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]
    if number == 0:
        points.append((8, 8, 8))  # Unused vertex; UV2 still examined there.
    position, _ = attribute(points, 'VEC3')
    normal, _ = attribute([(0, 0, 1)] * len(points), 'VEC3')
    attrs = {'POSITION': position, 'NORMAL': normal}
    if number < 2:
        uv2, _ = attribute([(0, 0), (1, 0), (0, 1)] + ([(-1, 2)] if number == 0 else []), 'VEC2')
        attrs['TEXCOORD_2'] = uv2
    if number == 0:
        uv0, bad_offset = attribute([(0.5, 0.5)] * 4, 'VEC2')
        attrs['TEXCOORD_0'] = uv0
    indices = [0, 1, 2] + ([0, 0, 1] if number == 0 else [])
    binary.extend(b'\0' * (-len(binary) % 4))
    offset = len(binary)
    binary.extend(struct.pack('<' + 'H' * len(indices), *indices))
    views.append({'buffer': 0, 'byteOffset': offset, 'byteLength': len(indices) * 2})
    accessors.append({'bufferView': len(views) - 1, 'componentType': 5123, 'count': len(indices), 'type': 'SCALAR'})
    meshes.append({'primitives': [{'attributes': attrs, 'indices': len(accessors) - 1}]})

nodes = [{'name': name, 'mesh': mesh, 'translation': [x, y, 0]} for name, mesh, x, y in [
    ('Collapsed UV0 and unused outside UV2', 0, -1.5, 0),
    ('UV2 without UV0', 1, 0.5, 0),
    ('No source UV channels', 2, -1.5, -1.5),
    ('Instance of first source primitive', 0, 0.5, -1.5)]]
doc = {'asset': {'version': '2.0', 'generator': 'Asset Doctor authored source UV fixture'},
       'buffers': [{'byteLength': len(binary)}], 'bufferViews': views, 'accessors': accessors,
       'meshes': meshes, 'nodes': nodes, 'scenes': [{'nodes': list(range(4))}], 'scene': 0}
text = json.dumps(doc, separators=(',', ':')).encode()
text += b' ' * (-len(text) % 4)
binary.extend(b'\0' * (-len(binary) % 4))
manifest = root / 'manifest.json'
names = {'UV_Checker.glb', 'Bad_Unused_UV.glb'}
rows = [r for r in json.loads(manifest.read_text(encoding='utf-8')) if r['file'] not in names]
for name, invalid in [('UV_Checker.glb', False), ('Bad_Unused_UV.glb', True)]:
    data = bytearray(binary)
    if invalid:
        struct.pack_into('<f', data, bad_offset, float('nan'))
    payload = struct.pack('<IIIII', 0x46546c67, 2, 28 + len(text) + len(data), len(text), 0x4e4f534a) + text
    payload += struct.pack('<II', len(data), 0x004e4942) + data
    (root / name).write_bytes(payload)
    rows.append({'file': name, 'bytes': len(payload), 'sha256': hashlib.sha256(payload).hexdigest(),
                 'origin': 'Project-authored numeric source UV/geometry; no external content'})
manifest.write_text(json.dumps(sorted(rows, key=lambda r: r['file']), indent=2) + '\n', encoding='utf-8')
print('Generated UV_Checker and invalid unused UV:3source primitives/4triangles;6display triangles;9findings')
