#version 450
// ADR-0040 skybox: the owner's equirectangular panorama (2:1) behind the
// model. Same world ray as sky.frag; yaw (ground.w, radians) turns it around Y.
layout(location=0) in vec2 ndc;
layout(location=0) out vec4 color;
layout(set=0,binding=0) uniform texture2D panorama_image;
layout(set=0,binding=1) uniform sampler panorama_sampler;
layout(push_constant) uniform Push { mat4 inverse_view_projection; vec4 zenith; vec4 horizon; vec4 ground; } pc;
const float PI=3.14159265359;
void main() {
    vec4 near_point=pc.inverse_view_projection*vec4(ndc,1.0,1.0);
    vec4 far_point=pc.inverse_view_projection*vec4(ndc,0.0,1.0);
    vec3 dir=normalize(far_point.xyz/far_point.w-near_point.xyz/near_point.w);
    float c=cos(pc.ground.w);
    float s=sin(pc.ground.w);
    dir=vec3(c*dir.x-s*dir.z,dir.y,s*dir.x+c*dir.z);
    vec2 uv=vec2(atan(dir.x,-dir.z)/(2.0*PI)+0.5,acos(clamp(dir.y,-1.0,1.0))/PI);
    // Level 0 only: derivatives jump at the wrap seam and would pick a blurry mip.
    vec3 rgb=textureLod(sampler2D(panorama_image,panorama_sampler),uv,0.0).rgb;
    // Image values are display colours; decode when the swapchain encodes sRGB.
    if (pc.zenith.w>0.5) {rgb=mix(rgb/12.92,pow((rgb+0.055)/1.055,vec3(2.4)),greaterThan(rgb,vec3(0.04045)));}
    color=vec4(rgb,1.0);
}
