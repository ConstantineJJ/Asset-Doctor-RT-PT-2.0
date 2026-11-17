"""Authored duplicate-index variant of the project's numeric normal-map fixture."""
from pathlib import Path
import hashlib
import json
import struct

root=Path(__file__).resolve().parents[1]/'tests'/'fixtures'
original=(root/'Normal_Checker.glb').read_bytes()
json_size,kind=struct.unpack_from('<II',original,12)
assert kind==0x4e4f534a
doc=json.loads(original[20:20+json_size])
bin_size,kind=struct.unpack_from('<II',original,20+json_size)
assert kind==0x004e4942
binary=bytearray(original[28+json_size:28+json_size+bin_size])
primitive=doc['meshes'][0]['primitives'][0]
accessor=doc['accessors'][primitive['indices']]
view=doc['bufferViews'][accessor['bufferView']]
component={5121:'B',5123:'H',5125:'I'}[accessor['componentType']]
assert not view.get('byteStride') and not accessor.get('normalized')
offset=view.get('byteOffset',0)+accessor.get('byteOffset',0)
indices=list(struct.unpack_from('<'+component*accessor['count'],binary,offset))
first=indices[:3]
for i in range(32):
    rotation=i%3
    indices.extend(first[rotation:]+first[:rotation])
binary.extend(b'\0'*(-len(binary)%4));offset=len(binary)
encoded=struct.pack('<'+'I'*len(indices),*indices);binary.extend(encoded)
doc['bufferViews'].append({'buffer':0,'byteOffset':offset,'byteLength':len(encoded),'target':34963})
doc['accessors'].append({'bufferView':len(doc['bufferViews'])-1,'componentType':5125,'count':len(indices),'type':'SCALAR'})
primitive['indices']=len(doc['accessors'])-1
doc['buffers'][0]['byteLength']=len(binary)
doc['asset']['generator']='Asset Doctor authored duplicate Heal fixture:32rotated copies'
text=json.dumps(doc,separators=(',',':')).encode();text+=b' '*(-len(text)%4)
binary.extend(b'\0'*(-len(binary)%4))
payload=struct.pack('<III',0x46546c67,2,28+len(text)+len(binary))+struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(binary),0x004e4942)+binary
output=root/'Duplicate_Heal.glb';output.write_bytes(payload)
rows={row['file']:row for row in json.loads((root/'manifest.json').read_text(encoding='utf-8'))}
rows[output.name]={'file':output.name,'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest(),'origin':'Project-authored numeric normal/tangent/UV/PNG fixture with32rotated duplicate triangles; no external content'}
(root/'manifest.json').write_text(json.dumps([rows[name] for name in sorted(rows)],indent=2)+'\n',encoding='utf-8')
assert (root/'Normal_Checker.glb').read_bytes()==original
print(f'Authored {output.name}: {len(payload)}bytes,32duplicates; source fixture unchanged')
