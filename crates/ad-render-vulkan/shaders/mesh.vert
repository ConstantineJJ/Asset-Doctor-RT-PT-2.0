#version 450
layout(location=0) in vec3 position;
layout(location=1) in vec2 tex_coord;
layout(location=2) in vec4 vertex_color;
layout(location=3) in vec3 normal;
layout(location=4) in vec2 metallic_roughness_coord;
layout(location=5) in vec2 normal_coord;
layout(location=6) in vec4 tangent;
layout(location=7) in vec2 occlusion_coord;
layout(location=0) out vec3 world_position;
layout(location=1) out vec2 uv;
layout(location=2) out vec4 tint;
layout(location=3) out vec3 world_normal;
layout(location=4) out vec2 mr_uv;
layout(location=5) out vec2 normal_uv;
layout(location=6) out vec4 world_tangent;
layout(location=7) out vec2 ao_uv;
layout(set=0,binding=0,std140) uniform Frame { mat4 view_projection; vec4 eye_exposure; vec4 lighting; } frame;
layout(push_constant) uniform Push { vec4 base_color; vec4 material; vec4 view; vec4 emissive; } pc;
void main() {
    world_position=position;
    uv=tex_coord;
    tint=vertex_color;
    world_normal=normal;
    mr_uv=metallic_roughness_coord;
    normal_uv=normal_coord;
    world_tangent=tangent;
    ao_uv=occlusion_coord;
    gl_Position=frame.view_projection*vec4(position,1.0);
}
