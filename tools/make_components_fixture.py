"""Project-authored vertex-index components, point connection and excluded data."""
from pathlib import Path
import hashlib
import json
import struct

root=Path(__file__).resolve().parents[1]/'tests'/'fixtures'
binary=bytearray();views=[];accessors=[];meshes=[]
def accessor(data,component,count,kind,**extra):
    binary.extend(b'\0'*(-len(binary)%4));offset=len(binary);binary.extend(data)
    views.append({'buffer':0,'byteOffset':offset,'byteLength':len(data)})
    accessors.append({'bufferView':len(views)-1,'componentType':component,'count':count,'type':kind,**extra})
    return len(accessors)-1
def mesh(points,indices):
    values=[v for p in points for v in p]
    pos=accessor(struct.pack('<'+'f'*len(values),*values),5126,len(points),'VEC3',min=[min(p[i] for p in points) for i in range(3)],max=[max(p[i] for p in points) for i in range(3)])
    idx=accessor(struct.pack('<'+'H'*len(indices),*indices),5123,len(indices),'SCALAR')
    meshes.append({'primitives':[{'attributes':{'POSITION':pos},'indices':idx}]})
points=[]
for x,y in [(-1.5,0),(0,0),(-1.5,1.5),(0,1.5)]:
    points.extend([(x,y,0),(x+1,y,0),(x,y+1,0)])
points.append((9,9,9)) # Unused source vertex, not an extra component or fit target.
mesh(points,list(range(12)))
mesh([(0,0,0),(1,0,0),(0,1,0),(0,-1,0),(-1,0,0)],[0,1,2,0,3,4])
mesh([(0,0,0),(1,0,0)],[0,0,1])
nodes=[{'name':'Four index components','mesh':0,'translation':[-2,0,0]},
       {'name':'Point connected','mesh':1,'translation':[1,0,0]},
       {'name':'Excluded repeated index','mesh':2,'translation':[1,-2,0]},
       {'name':'Instance of four groups','mesh':0,'translation':[3,0,0]}]
doc={'asset':{'version':'2.0','generator':'Asset Doctor authored connected-component fixture'},'buffers':[{'byteLength':len(binary)}],'bufferViews':views,'accessors':accessors,'meshes':meshes,'nodes':nodes,'scenes':[{'nodes':list(range(4))}],'scene':0}
text=json.dumps(doc,separators=(',',':')).encode();text+=b' '*(-len(text)%4)
binary.extend(b'\0'*(-len(binary)%4))
payload=struct.pack('<III',0x46546c67,2,28+len(text)+len(binary))+struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(binary),0x004e4942)+binary
path=root/'Components_Checker.glb';path.write_bytes(payload)
manifest=root/'manifest.json'
rows=[r for r in json.loads(manifest.read_text(encoding='utf-8')) if r['file']!=path.name]
rows.append({'file':path.name,'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest(),'origin':'Project-authored numeric connected-component/index data; no external content'})
manifest.write_text(json.dumps(sorted(rows,key=lambda r:r['file']),indent=2)+'\n',encoding='utf-8')
print(f'Generated {path.name}:3source primitives/7triangles/8findings;4groups in primitive0;11displayed triangles')
