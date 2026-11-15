#version 450
layout(location=0) in vec3 position;
layout(location=1) in vec2 tex_coord;
layout(location=2) in vec4 vertex_color;
layout(location=3) in vec3 normal;
layout(location=4) in vec2 metallic_roughness_coord;
layout(location=0) out vec3 world_position;
layout(location=1) out vec2 uv;
layout(location=2) out vec4 tint;
layout(location=3) out vec3 world_normal;
layout(location=4) out vec2 mr_uv;
layout(push_constant) uniform Push { mat4 view_projection; vec4 base_color; vec4 eye_exposure; vec4 material; vec4 lighting; } pc;
void main() {
    world_position=position;
    uv=tex_coord;
    tint=vertex_color;
    world_normal=normal;
    mr_uv=metallic_roughness_coord;
    gl_Position=pc.view_projection*vec4(position,1.0);
}
