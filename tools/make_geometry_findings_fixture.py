"""Authored diagnostic data: indexed defects, normals and shared instances."""
from pathlib import Path
import hashlib
import json
import struct

root=Path(__file__).resolve().parents[1]/'tests'/'fixtures'
binary=bytearray();views=[];accessors=[]
def accessor(data,component,count,kind,**extra):
    binary.extend(b'\0'*(-len(binary)%4));offset=len(binary);binary.extend(data)
    views.append({'buffer':0,'byteOffset':offset,'byteLength':len(data)})
    accessors.append({'bufferView':len(views)-1,'componentType':component,'count':count,'type':kind,**extra})
    return len(accessors)-1
positions=[0,0,0, 1,0,0, 0,1,0, 2,0,0, 0,0,1]
position=accessor(struct.pack('<15f',*positions),5126,5,'VEC3',min=[0,0,0],max=[2,1,1])
indices=accessor(struct.pack('<15H',0,1,2, 1,2,0, 0,2,1, 0,1,3, 0,0,1),5123,15,'SCALAR')
simple=accessor(struct.pack('<3H',0,1,2),5123,3,'SCALAR')
normal=accessor(struct.pack('<15f',*([0]*15)),5126,5,'VEC3')
doc={'asset':{'version':'2.0','generator':'Asset Doctor authored geometry diagnostic fixture'},
     'buffers':[{'byteLength':len(binary)}],'bufferViews':views,'accessors':accessors,
     'meshes':[{'primitives':[{'attributes':{'POSITION':position},'indices':indices}]},
               {'primitives':[{'attributes':{'POSITION':position,'NORMAL':normal},'indices':simple}]}],
     'nodes':[{'name':'Known defects','mesh':0,'translation':[-1.5,0,0]},
              {'name':'Zero normals','mesh':1,'translation':[1.5,0,0]},
              {'name':'Shared defects instance','mesh':0,'translation':[-1.5,2,0]}],
     'scenes':[{'nodes':[0,1,2]}],'scene':0}
text=json.dumps(doc,separators=(',',':')).encode();text+=b' '*(-len(text)%4)
binary.extend(b'\0'*(-len(binary)%4))
payload=struct.pack('<III',0x46546c67,2,28+len(text)+len(binary))+struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(binary),0x004e4942)+binary
(root/'Geometry_Findings.glb').write_bytes(payload)
rows=json.loads((root/'manifest.json').read_text(encoding='utf-8'))
rows=[row for row in rows if row['file']!='Geometry_Findings.glb']
rows.append({'file':'Geometry_Findings.glb','bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest(),'origin':'Project-authored numeric geometry/indices/normals; no external content'})
(root/'manifest.json').write_text(json.dumps(sorted(rows,key=lambda row:row['file']),indent=2)+'\n',encoding='utf-8')
print('Generated Geometry_Findings: 2 source primitives/6 triangles/6 findings; 3 instances/11 displayed triangles')
