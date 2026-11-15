"""Project-authored mathematical test geometry. No external asset content."""
import hashlib,json,struct
from pathlib import Path
root=Path(__file__).resolve().parents[1]/'tests'/'fixtures'
root.mkdir(parents=True,exist_ok=True)
positions=[[-1,-1,-1],[1,-1,-1],[1,1,-1],[-1,1,-1],[-1,-1,1],[1,-1,1],[1,1,1],[-1,1,1]]
indices=[0,2,1,0,3,2,4,5,6,4,6,7,0,1,5,0,5,4,2,3,7,2,7,6,1,2,6,1,6,5,3,0,4,3,4,7]
binary=b''.join(struct.pack('<3f',*p) for p in positions)+struct.pack('<36H',*indices)
doc={'asset':{'version':'2.0','generator':'Asset Doctor RT-PT synthetic fixture'},
     'buffers':[{'byteLength':len(binary)}],
     'bufferViews':[{'buffer':0,'byteOffset':0,'byteLength':96},{'buffer':0,'byteOffset':96,'byteLength':72}],
     'accessors':[{'bufferView':0,'componentType':5126,'count':8,'type':'VEC3','min':[-1,-1,-1],'max':[1,1,1]},
                  {'bufferView':1,'componentType':5123,'count':36,'type':'SCALAR'}],
     'materials':[{'pbrMetallicRoughness':{'baseColorFactor':c}} for c in [[.08,.5,.85,1],[.9,.22,.06,1]]],
     'meshes':[{'primitives':[{'attributes':{'POSITION':0},'indices':1,'material':i}]} for i in range(2)],
     'nodes':[{'name':'Synthetic scene','children':[1,2]},
              {'name':'Blue cube','mesh':0,'translation':[-1.4,0,0]},
              {'name':'Orange cube','mesh':1,'translation':[1.4,.1,0],'scale':[.8,1.1,.8]}],
     'scenes':[{'nodes':[0]}],'scene':0}
def encode(d):
    text=json.dumps(d,separators=(',',':')).encode()
    text+=b' '*((-len(text))%4)
    data=binary+b'\0'*((-len(binary))%4)
    return struct.pack('<III',0x46546c67,2,28+len(text)+len(data))+struct.pack('<II',len(text),0x4e4f534a)+text+struct.pack('<II',len(data),0x004e4942)+data
valid=encode(doc)
(root/'Twin_Cubes.glb').write_bytes(valid)
(root/'Bad_Truncated.glb').write_bytes(valid[:-7])
bad_accessor=json.loads(json.dumps(doc))
bad_accessor['meshes'][0]['primitives'][0]['attributes']['POSITION']=999
(root/'Bad_Accessor.glb').write_bytes(encode(bad_accessor))
manifest=[{'file':p.name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),
           'origin':'Project-authored synthetic mathematical geometry; no external models or textures'} for p in sorted(root.glob('*.glb'))]
(root/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest))
