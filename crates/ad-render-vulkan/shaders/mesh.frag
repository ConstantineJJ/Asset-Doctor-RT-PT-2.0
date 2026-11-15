#version 450
layout(location=0) in vec3 world_position;
layout(location=1) in vec2 uv;
layout(location=2) in vec4 tint;
layout(location=3) in vec3 world_normal;
layout(location=4) in vec2 mr_uv;
layout(set=0,binding=0) uniform texture2D base_color_image;
layout(set=0,binding=1) uniform sampler base_color_sampler;
layout(set=1,binding=0) uniform texture2D metallic_roughness_image;
layout(set=1,binding=1) uniform sampler metallic_roughness_sampler;
layout(location=0) out vec4 color;
layout(push_constant) uniform Push { mat4 view_projection; vec4 base_color; vec4 eye_exposure; vec4 material; vec4 lighting; } pc;
const float PI=3.14159265359;
// Scale first so tiny/large derivative normals do not underflow/overflow in dot.
vec3 unit_or(vec3 v,vec3 fallback) {
    float scale=max(max(abs(v.x),abs(v.y)),abs(v.z));
    if (scale<1e-30) {return fallback;}
    vec3 scaled=v/scale;
    return scaled*inversesqrt(dot(scaled,scaled));
}
vec3 direct_pbr(vec3 base,vec3 face,vec2 mr) {
    vec3 v=unit_or(pc.eye_exposure.xyz-world_position,vec3(0.0,0.0,1.0));
    vec3 n=unit_or(world_normal,unit_or(face,vec3(0.0,1.0,0.0)));
    if (dot(n,v)<0.0) {n=-n;}
    vec3 l=normalize(vec3(0.4,0.8,0.3));
    vec3 h=unit_or(v+l,n);
    float nv=max(dot(n,v),0.00001);
    float nl=max(dot(n,l),0.0);
    float nh=max(dot(n,h),0.0);
    float vh=clamp(dot(v,h),0.0,1.0);
    float metal=mr.x;
    float rough=max(mr.y,0.045);
    float alpha=rough*rough;
    float a2=alpha*alpha;
    vec3 f0=mix(vec3(0.04),base,metal);
    vec3 fresnel=f0+(vec3(1.0)-f0)*pow(1.0-vh,5.0);
    float denominator=1.0+nh*nh*(a2-1.0);
    float distribution=a2/(PI*denominator*denominator);
    float visibility=0.5/max(nl*sqrt(nv*nv*(1.0-a2)+a2)+nv*sqrt(nl*nl*(1.0-a2)+a2),0.000001);
    vec3 diffuse=(vec3(1.0)-fresnel)*(1.0-metal)*base/PI;
    vec3 specular=fresnel*distribution*visibility;
    // Inspection fill is an explicit approximation; no environment is sampled.
    vec3 radiance=(diffuse+specular)*nl*pc.lighting.x+((1.0-metal)*base+f0)*pc.lighting.y;
    vec3 exposed=max(radiance*exp2(pc.eye_exposure.w),vec3(0.0));
    return exposed/(vec3(1.0)+exposed);
}
void main() {
    vec3 face=cross(dFdx(world_position),dFdy(world_position));
    vec3 n=face;
    n/=max(length(n),0.00000001);
    float light=0.25+0.75*abs(dot(n,normalize(vec3(0.4,0.8,0.3))));
    vec4 base=pc.base_color*tint;
    uint flags=uint(pc.lighting.z);
    if ((flags&2u)!=0u) {base*=texture(sampler2D(base_color_image,base_color_sampler),uv);}
    if (pc.material.z>=0.0 && base.a<pc.material.z) {discard;}
    vec3 linear=max(base.rgb*light,vec3(0.0));
    if ((flags&1u)!=0u) {
        vec2 mr=pc.material.xy;
        if ((flags&8u)!=0u) {
            // glTF data texture is linear: B=metal, G=roughness; ignore R/A.
            mr*=texture(sampler2D(metallic_roughness_image,metallic_roughness_sampler),mr_uv).bg;
        }
        linear=direct_pbr(base.rgb,face,mr);
    }
    vec3 display=linear;
    if ((flags&4u)!=0u) {
        display=mix(12.92*linear,1.055*pow(linear,vec3(1.0/2.4))-0.055,greaterThan(linear,vec3(0.0031308)));
    }
    color=vec4(display,1.0);
}
