#version 450
// Outline mask: red = selected, green = hovered (blended with MAX).
layout(location=0) out vec4 color;
layout(push_constant) uniform Push { mat4 view_projection; vec4 value; } pc;
void main() {
    color=pc.value;
}
