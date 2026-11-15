#version 450
layout(location=0) in vec3 position;
layout(location=0) out vec3 world_position;
layout(push_constant) uniform Push { mat4 view_projection; vec4 base_color; uint encode_srgb; } pc;
void main() {
    world_position=position;
    gl_Position=pc.view_projection*vec4(position,1.0);
}
