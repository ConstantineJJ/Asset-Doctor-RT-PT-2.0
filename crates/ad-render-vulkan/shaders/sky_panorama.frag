#version 450
// ADR-0040/0047 skybox: the owner's equirectangular panorama (2:1) behind the
// model, stored as linear radiance (RGBA16F). Same world ray as sky.frag; yaw
// (ground.w, radians) turns it around Y. HDR panoramas (horizon.x) are shown
// at the exposure horizon.y (EV) with the mesh's Reinhard curve; LDR ones
// exactly as their image.
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
    vec3 rgb=max(textureLod(sampler2D(panorama_image,panorama_sampler),uv,0.0).rgb,vec3(0.0));
    if (pc.horizon.x>0.5) {
        vec3 exposed=rgb*exp2(pc.horizon.y);
        rgb=exposed/(vec3(1.0)+exposed);
    }
    // Linear values; encode here unless the swapchain encodes sRGB itself.
    if (pc.zenith.w<0.5) {rgb=mix(12.92*rgb,1.055*pow(rgb,vec3(1.0/2.4))-0.055,greaterThan(rgb,vec3(0.0031308)));}
    color=vec4(rgb,1.0);
}
