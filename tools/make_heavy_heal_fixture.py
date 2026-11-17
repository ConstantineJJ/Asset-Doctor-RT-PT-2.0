"""Project-authored heavy G002 measurement input; written only to ignored artifacts/.

A flat grid (~1.5M triangles) with POSITION/NORMAL/TEXCOORD_0, a generated
2048x2048 RGBA checker base-colour texture and 32000 rotated duplicate copies
of early triangles. Sized so all 11 analysis rules stay within default limits.
"""
from pathlib import Path
import json
import struct
import zlib

repo = Path(__file__).resolve().parents[1]
side = 868  # vertices per side -> 867x867 quads -> 1,503,378 triangles
duplicates = 32000
texture = 2048

positions = bytearray()
normals = bytearray()
uvs = bytearray()
for y in range(side):
    for x in range(side):
        positions += struct.pack('<3f', x / (side - 1), y / (side - 1), 0.0)
        normals += struct.pack('<3f', 0.0, 0.0, 1.0)
        uvs += struct.pack('<2f', x / (side - 1), y / (side - 1))
faces = []
for y in range(side - 1):
    for x in range(side - 1):
        a = y * side + x
        faces.append((a, a + 1, a + side + 1))
        faces.append((a, a + side + 1, a + side))
for i in range(duplicates):
    face = faces[i % 1000]
    r = i % 3
    faces.append(face[r:] + face[:r])
indices = bytearray()
for face in faces:
    indices += struct.pack('<3I', *face)

raw = bytearray()
row_flat = bytearray()
for x in range(texture):
    row_flat += bytes((230, 230, 230, 255)) if (x // 128) % 2 == 0 else bytes((200, 40, 50, 255))
alt_flat = bytearray()
for x in range(texture):
    alt_flat += bytes((200, 40, 50, 255)) if (x // 128) % 2 == 0 else bytes((230, 230, 230, 255))
for y in range(texture):
    raw += b'\0' + (row_flat if (y // 128) % 2 == 0 else alt_flat)


def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff)


png = (b'\x89PNG\r\n\x1a\n'
       + chunk(b'IHDR', struct.pack('>IIBBBBB', texture, texture, 8, 6, 0, 0, 0))
       + chunk(b'IDAT', zlib.compress(bytes(raw), 6))
       + chunk(b'IEND', b''))

binary = bytearray()
views = []


def add(data, target=None):
    binary.extend(b'\0' * (-len(binary) % 4))
    view = {'buffer': 0, 'byteOffset': len(binary), 'byteLength': len(data)}
    if target:
        view['target'] = target
    binary.extend(data)
    views.append(view)
    return len(views) - 1


count = side * side
p = add(positions, 34962)
n = add(normals, 34962)
t = add(uvs, 34962)
i = add(indices, 34963)
img = add(png)
binary.extend(b'\0' * (-len(binary) % 4))
doc = {
    'asset': {'version': '2.0', 'generator': 'Asset Doctor authored heavy G002 measurement input'},
    'buffers': [{'byteLength': len(binary)}],
    'bufferViews': views,
    'accessors': [
        {'bufferView': p, 'componentType': 5126, 'count': count, 'type': 'VEC3', 'min': [0, 0, 0], 'max': [1, 1, 0]},
        {'bufferView': n, 'componentType': 5126, 'count': count, 'type': 'VEC3'},
        {'bufferView': t, 'componentType': 5126, 'count': count, 'type': 'VEC2'},
        {'bufferView': i, 'componentType': 5125, 'count': len(faces) * 3, 'type': 'SCALAR'},
    ],
    'images': [{'bufferView': img, 'mimeType': 'image/png'}],
    'samplers': [{}],
    'textures': [{'source': 0, 'sampler': 0}],
    'materials': [{'pbrMetallicRoughness': {'baseColorTexture': {'index': 0}}}],
    'meshes': [{'primitives': [{'attributes': {'POSITION': 0, 'NORMAL': 1, 'TEXCOORD_0': 2}, 'indices': 3, 'material': 0}]}],
    'nodes': [{'mesh': 0}],
    'scenes': [{'nodes': [0]}],
    'scene': 0,
}
text = json.dumps(doc, separators=(',', ':')).encode()
text += b' ' * (-len(text) % 4)
payload = (struct.pack('<III', 0x46546c67, 2, 28 + len(text) + len(binary))
           + struct.pack('<II', len(text), 0x4e4f534a) + text
           + struct.pack('<II', len(binary), 0x004e4942) + binary)
out = repo / 'artifacts' / 'Heavy_Heal.glb'
out.parent.mkdir(exist_ok=True)
out.write_bytes(payload)
print(f'{out.name} bytes={len(payload)} vertices={count} triangles={len(faces)} duplicates={duplicates}')
