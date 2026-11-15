"""Project-authored emissive factors/UV/sharing/coverage cases; no external assets."""
from pathlib import Path
import copy
import hashlib
import json
import struct

root=Path(__file__).resolve().parents[1]/'tests'/'fixtures'
source=(root/'MR_Checker.glb').read_bytes()
length=struct.unpack_from('<I',source,12)[0]
doc=json.loads(source[20:20+length])
binary=bytearray(source[28+length:])
doc['asset']['generator']='Asset Doctor authored emission fixture'
def append_view(data):
    binary.extend(b'\0'*(-len(binary)%4))
    offset=len(binary);binary.extend(data)
    doc['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(data)})
    return len(doc['bufferViews'])-1
count=doc['accessors'][0]['count']
values=[v for i in range(count) for v in [i//33/16,i%33/32]]
doc['accessors'].append({'bufferView':append_view(struct.pack('<'+'f'*len(values),*values)),'componentType':5126,'count':count,'type':'VEC2'})
uv=len(doc['accessors'])-1
material=copy.deepcopy(doc['materials'][0]);primitive=copy.deepcopy(doc['meshes'][0]['primitives'][0])
doc['meshes'],doc['materials'],doc['nodes']=[],[],[]
for i,factor in enumerate([[1,.5,.25],[0,0,0],[.2,.6,1],[1,1,1]]):
    m=copy.deepcopy(material);m['emissiveFactor']=factor
    if i!=2: m['emissiveTexture']={'index':0,'texCoord':2}
    if i==3:
        m['alphaMode']='MASK';m['alphaCutoff']=.5
        m['pbrMetallicRoughness']['baseColorTexture']['texCoord']=1
    p=copy.deepcopy(primitive);p['material']=i;p['attributes']['TEXCOORD_2']=uv
    doc['materials'].append(m);doc['meshes'].append({'primitives':[p]})
    doc['nodes'].append({'name':['SRGB emission','Default dark factor0','Factor only','Base MASK controls coverage'][i],'mesh':i,'translation':[(i%2-.5)*1.6,(.5-i//2)*1.6,0]})
doc['scenes']=[{'nodes':list(range(4))}]
def encode(metadata,payload):
    metadata['buffers']=[{'byteLength':len(payload)}]
    text=json.dumps(metadata,separators=(',',':')).encode();text+=b' '*(-len(text)%4);payload+=b'\0'*(-len(payload)%4)
    return struct.pack('<III',0x46546c67,2,28+len(text)+len(payload))+struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(payload),0x004e4942)+payload
(root/'Emission_Checker.glb').write_bytes(encode(copy.deepcopy(doc),bytes(binary)))
bad=copy.deepcopy(doc)
for m in bad['materials']:
    m['pbrMetallicRoughness'].pop('baseColorTexture',None);m['pbrMetallicRoughness'].pop('metallicRoughnessTexture',None)
bad['images'][0]['bufferView']=append_view(b'bad PNG');bad['bufferViews']=copy.deepcopy(doc['bufferViews'])
(root/'Bad_Emission_Image.glb').write_bytes(encode(bad,bytes(binary)))
old={x['file']:x for x in json.loads((root/'manifest.json').read_text(encoding='utf-8'))}
rows=[{'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'origin':old.get(p.name,{}).get('origin','Project-authored numeric emission/UV/PNG data; no external content')} for p in sorted(root.glob('*.glb'))]
(root/'manifest.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
print('Generated emission fixtures: shared SRGB, independent UV2, factor-only, MASK')
