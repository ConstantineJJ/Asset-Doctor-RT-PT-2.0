"""Authored numeric color/UV test data; no external model or image content."""
from pathlib import Path
import copy
import hashlib
import json
import struct
import zlib

root = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures'
def chunk(kind, data):
    return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))

def png(width=2, height=2):
    pixels = bytes([255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255, 255, 0])
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b'\0' + pixels[:8] + b'\0' + pixels[8:])) + chunk(b'IEND', b''))

geometry = (struct.pack('<12f', -1, -1, 0, 1, -1, 0, 1, 1, 0, -1, 1, 0)
            + struct.pack('<8f', 0, 1, 1, 1, 1, 0, 0, 0)
            + struct.pack('<16f', 1, 1, 1, 1, .5, 1, .5, 1, 1, .5, 1, 1, 1, 1, 1, 1)
            + struct.pack('<6H', 0, 1, 2, 0, 2, 3))
doc = {
    'asset': {'version': '2.0', 'generator': 'Asset Doctor authored color fixture'},
    'bufferViews': [{'buffer': 0, 'byteOffset': o, 'byteLength': n} for o, n in [(0, 48), (48, 32), (80, 64), (144, 12)]],
    'accessors': [
        {'bufferView': 0, 'componentType': 5126, 'count': 4, 'type': 'VEC3', 'min': [-1, -1, 0], 'max': [1, 1, 0]},
        {'bufferView': 1, 'componentType': 5126, 'count': 4, 'type': 'VEC2'},
        {'bufferView': 2, 'componentType': 5126, 'count': 4, 'type': 'VEC4'},
        {'bufferView': 3, 'componentType': 5123, 'count': 6, 'type': 'SCALAR'}],
    'images': [{'bufferView': 4, 'mimeType': 'image/png'}],
    'samplers': [{'minFilter': 9987, 'magFilter': 9729, 'wrapS': 10497, 'wrapT': 33648},
                 {'minFilter': 9728, 'magFilter': 9728, 'wrapS': 33071, 'wrapT': 33071}],
    'textures': [{'source': 0, 'sampler': i} for i in range(2)],
    'materials': [{'pbrMetallicRoughness': {'baseColorTexture': {'index': i}, 'baseColorFactor': [.8, 1, 1, 1]},
                   'doubleSided': True, 'alphaMode': 'MASK' if i else 'OPAQUE', 'alphaCutoff': .5} for i in range(2)],
    'meshes': [{'primitives': [{'attributes': {'POSITION': 0, 'TEXCOORD_0': 1, 'COLOR_0': 2}, 'indices': 3, 'material': i}]} for i in range(2)],
    'nodes': [{'name': 'Opaque repeat', 'mesh': 0, 'translation': [-1.25, 0, 0]},
              {'name': 'Mask clamp', 'mesh': 1, 'translation': [1.25, 0, 0]}],
    'scenes': [{'nodes': [0, 1]}], 'scene': 0}

def encode(image):
    metadata = copy.deepcopy(doc)
    binary = geometry + image
    metadata['buffers'] = [{'byteLength': len(binary)}]
    metadata['bufferViews'].append({'buffer': 0, 'byteOffset': len(geometry), 'byteLength': len(image)})
    text = json.dumps(metadata, separators=(',', ':')).encode()
    text += b' ' * (-len(text) % 4)
    binary += b'\0' * (-len(binary) % 4)
    return (struct.pack('<III', 0x46546c67, 2, 28 + len(text) + len(binary))
            + struct.pack('<II', len(text), 0x4e4f534a) + text
            + struct.pack('<II', len(binary), 0x004e4942) + binary)

root.mkdir(parents=True, exist_ok=True)
for name, data in [('Color_Checker.glb', png()), ('Bad_Image.glb', b'bad PNG'), ('Huge_Image_Header.glb', png(32768, 32768))]:
    (root / name).write_bytes(encode(data))
manifest = [{'file': p.name, 'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest(),
             'origin': 'Project-authored numeric test geometry and PNG; no external content'}
            for p in sorted(root.glob('*.glb'))]
(root / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print('Generated color/mask/malformed fixtures and updated hashes')
