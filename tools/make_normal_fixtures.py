"""Project-authored normal/tangent/UV fixtures; no external model or texture."""
from pathlib import Path
import copy
import hashlib
import json
import math
import struct
import zlib

root = Path(__file__).resolve().parents[1] / 'tests' / 'fixtures'
source = (root/'MR_Checker.glb').read_bytes()
length = struct.unpack_from('<I',source,12)[0]
doc = json.loads(source[20:20+length])
binary = bytearray(source[28+length:])
doc['asset']['generator'] = 'Asset Doctor authored normal-map fixture'

def view(data):
    binary.extend(b'\0'*(-len(binary)%4))
    start = len(binary)
    binary.extend(data)
    doc['bufferViews'].append({'buffer':0,'byteOffset':start,'byteLength':len(data)})
    return len(doc['bufferViews'])-1

def accessor(rows, kind):
    data = struct.pack('<'+'f'*sum(map(len,rows)),*(v for row in rows for v in row))
    doc['accessors'].append({'bufferView':view(data),'componentType':5126,'count':len(rows),'type':kind})
    return len(doc['accessors'])-1

count = doc['accessors'][0]['count']
uv = accessor([[1-i%33/32,1-i//33/16] for i in range(count)],'VEC2')
flat_uv = accessor([[.5,.5]]*count,'VEC2')
tangent = accessor([[math.sin(2*math.pi*(i%33)/32),0,-math.cos(2*math.pi*(i%33)/32),1] for i in range(count)],'VEC4')

def chunk(kind,data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))

pixels = bytes([128,128,255,0,220,128,220,255,128,220,220,0,40,128,220,255])
png = (b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',2,2,8,6,0,0,0))
       +chunk(b'IDAT',zlib.compress(b'\0'+pixels[:8]+b'\0'+pixels[8:]))+chunk(b'IEND',b''))
doc['images'][0]['bufferView'] = view(png)
primitive = copy.deepcopy(doc['meshes'][0]['primitives'][0])
doc['meshes'],doc['materials'],doc['nodes'] = [],[],[]
for i in range(4):
    p = copy.deepcopy(primitive)
    p['material'] = i
    p['attributes']['TEXCOORD_2'] = flat_uv if i==2 else uv
    if i in [0,3]:
        p['attributes']['TANGENT'] = tangent
    if i==3:
        del p['attributes']['NORMAL']
    doc['meshes'].append({'primitives':[p]})
    doc['materials'].append({'pbrMetallicRoughness':{'baseColorFactor':[.6,.6,.6,1],
        'metallicFactor':.2,'roughnessFactor':.7,'metallicRoughnessTexture':{'index':0,'texCoord':1}},
        'normalTexture':{'index':0,'texCoord':2,'scale':[1,.5,0,-1][i]}})
    doc['nodes'].append({'name':['Authored tangent','Mirrored derivative','Degenerate UV / scale0','No NORMAL / negative scale'][i],
        'mesh':i,'translation':[(i%2-.5)*1.6,(.5-i//2)*1.6,0],'scale':[-1,.8,1.3] if i==1 else [1,1,1]})
doc['scenes'] = [{'nodes':list(range(4))}]

def encode(metadata,payload):
    metadata['buffers']=[{'byteLength':len(payload)}]
    text=json.dumps(metadata,separators=(',',':')).encode()
    text+=b' '*(-len(text)%4)
    payload+=b'\0'*(-len(payload)%4)
    return (struct.pack('<III',0x46546c67,2,28+len(text)+len(payload))+struct.pack('<II',len(text),0x4e4f534a)
            +text+struct.pack('<II',len(payload),0x004e4942)+payload)

(root/'Normal_Checker.glb').write_bytes(encode(copy.deepcopy(doc),bytes(binary)))
bad=copy.deepcopy(doc)
for m in bad['materials']:
    m['pbrMetallicRoughness'].pop('metallicRoughnessTexture')
bad['images'][0]['bufferView']=view(b'bad PNG')
bad['bufferViews']=copy.deepcopy(doc['bufferViews'])
(root/'Bad_Normal_Image.glb').write_bytes(encode(bad,bytes(binary)))
old={x['file']:x for x in json.loads((root/'manifest.json').read_text(encoding='utf-8'))}
rows=[{'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),
       'origin':old.get(p.name,{}).get('origin','Project-authored numeric normal/tangent/UV/PNG data; no external content')}
      for p in sorted(root.glob('*.glb'))]
(root/'manifest.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
print('Generated normal/tangent fixtures with UV2, mirrored and invalid-frame cases')
