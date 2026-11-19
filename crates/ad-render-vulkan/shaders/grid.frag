#version 450
// ADR-0040: anti-aliased world-aligned grid lines, stronger every Nth line,
// red X axis (z = 0) and blue Z axis (x = 0), fading towards the edge.
layout(location=0) in vec2 world_xz;
layout(location=0) out vec4 color;
layout(push_constant) uniform Push { mat4 view_projection; vec4 plane; vec4 style; } pc;
float lines(vec2 p, float step) {
    vec2 c=p/step;
    vec2 d=max(fwidth(c),vec2(0.000001));
    vec2 g=abs(fract(c-0.5)-0.5)/d;
    return 1.0-min(min(g.x,g.y),1.0);
}
void main() {
    float minor=lines(world_xz,pc.style.x);
    float major=lines(world_xz,pc.style.x*pc.style.y);
    vec2 d=max(fwidth(world_xz),vec2(0.000001));
    float x_axis=1.0-min(abs(world_xz.y)/(1.5*d.y),1.0);
    float z_axis=1.0-min(abs(world_xz.x)/(1.5*d.x),1.0);
    vec3 rgb=vec3(0.55,0.58,0.62);
    float alpha=max(minor*0.22,major*0.5);
    rgb=mix(rgb,vec3(0.85,0.25,0.25),x_axis);
    alpha=max(alpha,x_axis*0.85);
    rgb=mix(rgb,vec3(0.25,0.45,0.95),z_axis);
    alpha=max(alpha,z_axis*0.85);
    float r=length(world_xz-vec2(pc.plane.x,pc.plane.z));
    float fade=1.0-smoothstep(0.55*pc.plane.w,pc.plane.w,r);
    color=vec4(rgb,alpha*fade*pc.style.w);
}
