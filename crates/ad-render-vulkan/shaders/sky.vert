#version 450
// ADR-0040: one full-screen triangle behind everything (environment sky).
layout(location=0) out vec2 ndc;
void main() {
    int i=int(gl_VertexIndex);
    vec2 p=vec2(i==1?3.0:-1.0,i==2?3.0:-1.0);
    ndc=p;
    gl_Position=vec4(p,0.0,1.0);
}
