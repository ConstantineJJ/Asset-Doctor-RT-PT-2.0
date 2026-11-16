"""Project-authored edge cases; optional large zero-area input stays in artifacts."""
from pathlib import Path
import argparse
import hashlib
import json
import struct

repo=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--large-refusal',action='store_true')
args=parser.parse_args()
binary=bytearray();views=[];accessors=[];meshes=[];nodes=[]
def accessor(data,component,count,kind,**extra):
    binary.extend(b'\0'*(-len(binary)%4));offset=len(binary);binary.extend(data)
    views.append({'buffer':0,'byteOffset':offset,'byteLength':len(data)})
    accessors.append({'bufferView':len(views)-1,'componentType':component,'count':count,'type':kind,**extra})
    return len(accessors)-1
def mesh(name,points,face_sets,translation):
    values=[v for point in points for v in point]
    position=accessor(struct.pack('<'+'f'*len(values),*values),5126,len(points),'VEC3',
                      min=[min(p[i] for p in points) for i in range(3)],max=[max(p[i] for p in points) for i in range(3)])
    primitives=[]
    for indices in face_sets:
        count=len(indices)//2 if isinstance(indices,bytes) else len(indices)
        data=indices if isinstance(indices,bytes) else struct.pack('<'+'H'*count,*indices)
        index=accessor(data,5123,count,'SCALAR')
        primitives.append({'attributes':{'POSITION':position},'indices':index})
    nodes.append({'name':name,'mesh':len(meshes),'translation':translation})
    meshes.append({'primitives':primitives})
if args.large_refusal:
    # 2M collinear faces: eligible by index, no raster overdraw, core fits64MiB;
    # edge scratch72MB refuses before allocation. Do not commit the12MB input.
    mesh('Edge scratch refusal',[(0,0,0),(1,0,0),(2,0,0)],[struct.pack('<3H',0,1,2)*2_000_000],[0,0,0])
    path=repo/'artifacts'/'Edge_Limit.glb'
else:
    pts=[(0,0,0),(1,0,0),(0,1,0),(0,0,1),(1,1,1)]
    mesh('Open triangle',pts[:3],[[0,1,2]],[-2.5,2.5,0])
    mesh('Closed tetrahedron',pts[:4],[[0,1,2,0,1,3,0,2,3,1,2,3]],[0,2.5,0])
    mesh('Three incident faces',pts,[[0,1,2,0,1,3,0,1,4]],[2.5,2.5,0])
    mesh('Excluded repeated index',pts[:2],[[0,0,1]],[-2.5,0,0])
    mesh('Coincident split indices',pts[:3]*2,[[0,1,2,3,4,5]],[0,0,0])
    mesh('Separate primitives',pts[:4],[[0,1,2],[0,2,3]],[2.5,0,0])
    nodes.append({'name':'Instance of incident faces','mesh':2,'translation':[0,-2.5,0]})
    path=repo/'tests'/'fixtures'/'Edge_Topology.glb'
doc={'asset':{'version':'2.0','generator':'Asset Doctor authored edge incidence fixture'},
     'buffers':[{'byteLength':len(binary)}],'bufferViews':views,'accessors':accessors,
     'meshes':meshes,'nodes':nodes,'scenes':[{'nodes':list(range(len(nodes)))}],'scene':0}
text=json.dumps(doc,separators=(',',':')).encode();text+=b' '*(-len(text)%4)
binary.extend(b'\0'*(-len(binary)%4))
payload=struct.pack('<III',0x46546c67,2,28+len(text)+len(binary))+struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(binary),0x004e4942)+binary
path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(payload)
if not args.large_refusal:
    manifest=path.parent/'manifest.json'
    rows=[r for r in json.loads(manifest.read_text(encoding='utf-8')) if r['file']!=path.name]
    rows.append({'file':path.name,'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest(),'origin':'Project-authored numeric edge incidence/primitive/instance data; no external content'})
    manifest.write_text(json.dumps(sorted(rows,key=lambda r:r['file']),indent=2)+'\n',encoding='utf-8')
print(f'Generated {path.name}: {len(payload)}bytes')
