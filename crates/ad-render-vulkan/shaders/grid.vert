#version 450
// ADR-0040: one procedural floor quad; no vertex buffers.
layout(location=0) out vec2 world_xz;
layout(push_constant) uniform Push { mat4 view_projection; vec4 plane; vec4 style; vec4 color_a; vec4 color_b; } pc;
void main() {
    int i=int(gl_VertexIndex);
    float x=(i==1||i==2||i==4)?1.0:-1.0;
    float z=(i==2||i==4||i==5)?1.0:-1.0;
    vec2 xz=vec2(pc.plane.x,pc.plane.z)+vec2(x,z)*pc.plane.w;
    world_xz=xz;
    gl_Position=pc.view_projection*vec4(xz.x,pc.plane.y,xz.y,1.0);
}
