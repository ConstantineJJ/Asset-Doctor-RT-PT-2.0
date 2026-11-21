#version 450
// Outline mask: positions of the selected/hovered draws only.
layout(location=0) in vec3 position;
layout(push_constant) uniform Push { mat4 view_projection; vec4 value; } pc;
void main() {
    gl_Position=pc.view_projection*vec4(position,1.0);
}
