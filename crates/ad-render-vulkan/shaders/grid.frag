#version 450
// ADR-0040: procedural floor themes (style.z): 0 studio grid, 1 dark matte,
// 2 light studio, 3 checker, 4 tiles. Red X axis (z = 0) and blue Z axis
// (x = 0) on every theme; everything fades towards the edge.
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
    int theme=int(pc.style.z+0.5);
    vec3 rgb;
    float alpha;
    if (theme==1) {
        rgb=mix(vec3(0.10,0.11,0.12),vec3(0.20,0.21,0.23),major*0.6);
        alpha=0.94;
    } else if (theme==2) {
        rgb=mix(vec3(0.64,0.65,0.67),vec3(0.52,0.53,0.55),minor*0.6);
        alpha=0.96;
    } else if (theme==3) {
        vec2 cell=floor(world_xz/pc.style.x);
        float parity=mod(cell.x+cell.y,2.0);
        rgb=mix(vec3(0.20,0.21,0.23),vec3(0.42,0.43,0.46),parity);
        alpha=0.95;
    } else if (theme==4) {
        float grout=lines(world_xz,pc.style.x*2.0);
        rgb=mix(vec3(0.52,0.49,0.45),vec3(0.26,0.24,0.22),grout);
        alpha=0.96;
    } else {
        rgb=vec3(0.55,0.58,0.62);
        alpha=max(minor*0.22,major*0.5);
    }
    rgb=mix(rgb,vec3(0.85,0.25,0.25),x_axis);
    alpha=max(alpha,x_axis*0.85);
    rgb=mix(rgb,vec3(0.25,0.45,0.95),z_axis);
    alpha=max(alpha,z_axis*0.85);
    float r=length(world_xz-vec2(pc.plane.x,pc.plane.z));
    float fade=1.0-smoothstep(0.55*pc.plane.w,pc.plane.w,r);
    color=vec4(rgb,alpha*fade*pc.style.w);
}
