"""Extend the project-authored sphere with numeric PNG and independent UV streams."""
from pathlib import Path
import copy
import hashlib
import json
import struct
import zlib

root = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures'
source = (root / 'Pbr_Factors.glb').read_bytes()
json_length = struct.unpack_from('<I', source, 12)[0]
doc = json.loads(source[20:20+json_length])
binary = bytearray(source[28+json_length:])
doc['asset']['generator'] = 'Asset Doctor authored MR fixture'

def append_view(data):
    binary.extend(b'\0' * (-len(binary) % 4))
    offset = len(binary)
    binary.extend(data)
    doc['bufferViews'].append({'buffer': 0, 'byteOffset': offset, 'byteLength': len(data)})
    return len(doc['bufferViews'])-1

count = doc['accessors'][0]['count']
for uv in [[[.25, .25]]*count, [[i%33/32, 1-i//33/16] for i in range(count)]]:
    view = append_view(struct.pack('<'+'f'*count*2, *(v for row in uv for v in row)))
    doc['accessors'].append({'bufferView': view, 'componentType': 5126, 'count': count, 'type': 'VEC2'})
uv0, uv1 = len(doc['accessors'])-2, len(doc['accessors'])-1

def chunk(kind, data):
    return struct.pack('>I', len(data))+kind+data+struct.pack('>I', zlib.crc32(kind+data))

pixels = bytes([255,64,0,0, 0,128,255,255, 128,255,128,0, 255,0,255,255])
png = (b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR', struct.pack('>IIBBBBB',2,2,8,6,0,0,0))
       +chunk(b'IDAT',zlib.compress(b'\0'+pixels[:8]+b'\0'+pixels[8:]))+chunk(b'IEND',b''))
view = append_view(png)
doc['images'] = [{'bufferView': view, 'mimeType': 'image/png'}]
doc['samplers'] = [{'minFilter': 9987, 'magFilter': 9729, 'wrapS': 10497, 'wrapT': 33648}]
doc['textures'] = [{'source': 0, 'sampler': 0}]
doc['materials'] = []
doc['meshes'] = []
doc['nodes'] = []
for i, (metal, roughness) in enumerate([(1,1),(.5,.5),(1,1)]):
    pbr = {'metallicFactor': metal, 'roughnessFactor': roughness,
           'baseColorFactor': [1,1,1,1], 'metallicRoughnessTexture': {'index':0,'texCoord':1}}
    if i != 2:
        pbr['baseColorTexture'] = {'index':0,'texCoord':0}
    doc['materials'].append({'pbrMetallicRoughness':pbr, 'alphaMode':'OPAQUE'})
    doc['meshes'].append({'primitives':[{'attributes':{'POSITION':0,'NORMAL':1,'TEXCOORD_0':uv0,'TEXCOORD_1':uv1},'indices':2,'material':i}]})
    doc['nodes'].append({'name':['Shared SRGB + linear','Factors multiply map','MR only / white base'][i],'mesh':i,'translation':[(i-1)*1.6,0,0]})
doc['scenes'] = [{'nodes':[0,1,2]}]

def encode(metadata, payload):
    metadata['buffers'] = [{'byteLength':len(payload)}]
    text = json.dumps(metadata,separators=(',',':')).encode()
    text += b' '*(-len(text)%4)
    payload += b'\0'*(-len(payload)%4)
    return (struct.pack('<III',0x46546c67,2,28+len(text)+len(payload))
            +struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(payload),0x004e4942)+payload)

(root/'MR_Checker.glb').write_bytes(encode(copy.deepcopy(doc),bytes(binary)))
bad = copy.deepcopy(doc)
for material in bad['materials']:
    material['pbrMetallicRoughness'].pop('baseColorTexture',None)
bad['bufferViews'][view]['byteLength'] = 7
bad_binary = bytearray(binary)
start = bad['bufferViews'][view]['byteOffset']
bad_binary[start:start+7] = b'bad PNG'
(root/'Bad_MR_Image.glb').write_bytes(encode(bad,bytes(bad_binary)))
old = {x['file']:x for x in json.loads((root/'manifest.json').read_text(encoding='utf-8'))}
rows = [{'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),
         'origin':old.get(p.name,{}).get('origin','Project-authored numeric MR/UV/PNG data; no external content')}
        for p in sorted(root.glob('*.glb'))]
(root/'manifest.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
print('Generated MR_Checker and Bad_MR_Image; shared image, distinct transfer roles/UV sets')
