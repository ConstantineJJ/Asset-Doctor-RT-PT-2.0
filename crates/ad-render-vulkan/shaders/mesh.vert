#version 450
layout(location=0) in vec3 position;
layout(location=1) in vec2 tex_coord;
layout(location=2) in vec4 vertex_color;
layout(location=3) in vec3 normal;
layout(location=4) in vec2 metallic_roughness_coord;
layout(location=5) in vec2 normal_coord;
layout(location=6) in vec4 tangent;
layout(location=7) in vec2 occlusion_coord;
layout(location=8) in vec2 emissive_coord;
layout(location=0) out vec3 world_position;
layout(location=1) out vec2 uv;
layout(location=2) out vec4 tint;
layout(location=3) out vec3 world_normal;
layout(location=4) out vec2 mr_uv;
layout(location=5) out vec2 normal_uv;
layout(location=6) out vec4 world_tangent;
layout(location=7) out vec2 ao_uv;
layout(location=8) out vec2 emissive_uv;
layout(set=0,binding=0,std140) uniform Frame { mat4 view_projection; vec4 eye_exposure; vec4 lighting; vec4 sky_zenith; vec4 sky_horizon; vec4 sky_ground; vec4 light_direction; vec4 light_color; mat4 light_view_projection; vec4 shadow; vec4 ibl; vec4 sh[9]; mat4 node_preview; } frame;
layout(push_constant) uniform Push { vec4 base_color; vec4 material; vec4 view; vec4 emissive; } pc;
void main() {
    // ADR-0048: an object being dragged moves with the preview matrix.
    bool moved=pc.emissive.w>0.5;
    vec3 p=moved?(frame.node_preview*vec4(position,1.0)).xyz:position;
    world_position=p;
    uv=tex_coord;
    tint=vertex_color;
    world_normal=moved?mat3(frame.node_preview)*normal:normal;
    mr_uv=metallic_roughness_coord;
    normal_uv=normal_coord;
    world_tangent=moved?vec4(mat3(frame.node_preview)*tangent.xyz,tangent.w):tangent;
    ao_uv=occlusion_coord;
    emissive_uv=emissive_coord;
    gl_Position=frame.view_projection*vec4(p,1.0);
}
