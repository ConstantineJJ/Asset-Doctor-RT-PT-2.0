#version 450
// ADR-0044: model depth as seen from the lamp (orthographic light matrix).
layout(location=0) in vec3 position;
layout(push_constant) uniform Push { mat4 light_view_projection; } pc;
void main() {
    gl_Position=pc.light_view_projection*vec4(position,1.0);
}
