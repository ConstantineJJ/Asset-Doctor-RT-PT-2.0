#version 450
// ADR-0040: environment sky gradient by view direction. The inverse
// view-projection turns the pixel into a world ray (reversed-Z: near plane at
// depth 1, far plane at depth 0); up blends horizon → zenith, down → ground.
layout(location=0) in vec2 ndc;
layout(location=0) out vec4 color;
layout(push_constant) uniform Push { mat4 inverse_view_projection; vec4 zenith; vec4 horizon; vec4 ground; } pc;
void main() {
    vec4 near_point=pc.inverse_view_projection*vec4(ndc,1.0,1.0);
    vec4 far_point=pc.inverse_view_projection*vec4(ndc,0.0,1.0);
    vec3 dir=normalize(far_point.xyz/far_point.w-near_point.xyz/near_point.w);
    float t=dir.y;
    vec3 rgb;
    if (t>=0.0) {
        rgb=mix(pc.horizon.rgb,pc.zenith.rgb,pow(clamp(t,0.0,1.0),0.55));
    } else {
        rgb=mix(pc.horizon.rgb,pc.ground.rgb,pow(clamp(-t,0.0,1.0),0.35));
    }
    // Presets are display colours; decode when the swapchain encodes sRGB.
    if (pc.zenith.w>0.5) {rgb=mix(rgb/12.92,pow((rgb+0.055)/1.055,vec3(2.4)),greaterThan(rgb,vec3(0.04045)));}
    color=vec4(rgb,1.0);
}
