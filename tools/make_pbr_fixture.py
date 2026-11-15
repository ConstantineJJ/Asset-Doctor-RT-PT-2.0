"""Numeric smooth spheres for inspecting factor-only direct lighting. No external assets."""
from pathlib import Path
import hashlib
import json
import math
import struct

root = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures'
binary = bytearray()
views, accessors = [], []

def attribute(values, kind, components, component_type=5126):
    binary.extend(b'\0' * (-len(binary) % 4))
    start = len(binary)
    code = 'f' if component_type == 5126 else 'I'
    flat = [v for row in values for v in row] if components > 1 else values
    binary.extend(struct.pack('<' + code * len(flat), *flat))
    views.append({'buffer': 0, 'byteOffset': start, 'byteLength': len(binary)-start})
    entry = {'bufferView': len(views)-1, 'componentType': component_type, 'count': len(values), 'type': kind}
    if kind == 'VEC3':
        entry['min'] = [min(row[c] for row in values) for c in range(3)]
        entry['max'] = [max(row[c] for row in values) for c in range(3)]
    accessors.append(entry)
    return len(accessors)-1

normals, indices = [], []
segments, rings = 32, 16
for ring in range(rings+1):
    theta = math.pi * ring / rings
    for segment in range(segments+1):
        phi = 2*math.pi*segment/segments
        normals.append([math.sin(theta)*math.cos(phi), math.cos(theta), math.sin(theta)*math.sin(phi)])
for ring in range(rings):
    for segment in range(segments):
        a = ring*(segments+1)+segment
        b = a+segments+1
        if ring > 0:
            indices.extend([a, a+1, b])
        if ring < rings-1:
            indices.extend([a+1, b+1, b])
position = attribute([[c*.6 for c in n] for n in normals], 'VEC3', 3)
normal = attribute(normals, 'VEC3', 3)
index = attribute(indices, 'SCALAR', 1, 5125)
materials, meshes, nodes = [], [], []
for metal in [0, 1]:
    for column, roughness in enumerate([.15, .5, .95]):
        i = len(materials)
        materials.append({'name': f'Metal {metal} roughness {roughness}', 'pbrMetallicRoughness': {
            'baseColorFactor': [.8, .35, .08, 1], 'metallicFactor': metal, 'roughnessFactor': roughness}})
        meshes.append({'primitives': [{'attributes': {'POSITION': position, 'NORMAL': normal}, 'indices': index, 'material': i}]})
        nodes.append({'name': materials[-1]['name'], 'mesh': i, 'translation': [(column-1)*1.6, .85-metal*1.7, 0]})
# Two plane markers: no source normals, and singular transform with source normals.
p = attribute([[-.4, 0, -.3], [.4, 0, -.3], [0, 0, .4]], 'VEC3', 3)
n = attribute([[0, 1, 0]]*3, 'VEC3', 3)
for singular in [False, True]:
    attrs = {'POSITION': p}
    if singular:
        attrs['NORMAL'] = n
    meshes.append({'primitives': [{'attributes': attrs, 'material': 0}]})
    nodes.append({'name': 'Singular normal fallback' if singular else 'Missing normal fallback', 'mesh': len(meshes)-1,
                  'translation': [-.7 if singular else .7, -1.8, .3], 'scale': [1, 0 if singular else 1, 1]})
doc = {'asset': {'version': '2.0', 'generator': 'Asset Doctor authored normal/material fixture'},
       'buffers': [{'byteLength': len(binary)}], 'bufferViews': views, 'accessors': accessors,
       'materials': materials, 'meshes': meshes, 'nodes': nodes,
       'scenes': [{'nodes': list(range(len(nodes)))}], 'scene': 0}
text = json.dumps(doc, separators=(',', ':')).encode()
text += b' ' * (-len(text) % 4)
binary.extend(b'\0' * (-len(binary) % 4))
data = (struct.pack('<III', 0x46546c67, 2, 28+len(text)+len(binary)) + struct.pack('<II', len(text), 0x4e4f534a)
        + text + struct.pack('<II', len(binary), 0x004e4942) + binary)
(root / 'Pbr_Factors.glb').write_bytes(data)
old_manifest = {entry['file']: entry for entry in json.loads((root / 'manifest.json').read_text(encoding='utf-8'))}
manifest = [{'file': p.name, 'bytes': p.stat().st_size, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest(),
             'origin': old_manifest.get(p.name, {}).get('origin', 'Project-authored numeric test data; no external content')}
            for p in sorted(root.glob('*.glb'))]
(root / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n', encoding='utf-8')
print('Generated Pbr_Factors.glb with six smooth spheres and two fallback markers')
