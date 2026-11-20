#version 450
layout(location=0) in vec3 world_position;
layout(location=1) in vec2 uv;
layout(location=2) in vec4 tint;
layout(location=3) in vec3 world_normal;
layout(location=4) in vec2 mr_uv;
layout(location=5) in vec2 normal_uv;
layout(location=6) in vec4 world_tangent;
layout(location=7) in vec2 ao_uv;
layout(location=8) in vec2 emissive_uv;
layout(set=1,binding=0) uniform texture2D base_color_image;
layout(set=1,binding=1) uniform sampler base_color_sampler;
layout(set=1,binding=2) uniform texture2D metallic_roughness_image;
layout(set=1,binding=3) uniform sampler metallic_roughness_sampler;
layout(set=1,binding=4) uniform texture2D normal_image;
layout(set=1,binding=5) uniform sampler normal_sampler;
layout(set=1,binding=6) uniform texture2D occlusion_image;
layout(set=1,binding=7) uniform sampler occlusion_sampler;
layout(set=1,binding=8) uniform texture2D emissive_image;
layout(set=1,binding=9) uniform sampler emissive_sampler;
layout(location=0) out vec4 color;
layout(set=0,binding=0,std140) uniform Frame { mat4 view_projection; vec4 eye_exposure; vec4 lighting; vec4 sky_zenith; vec4 sky_horizon; vec4 sky_ground; vec4 light_direction; vec4 light_color; } frame;
layout(push_constant) uniform Push { vec4 base_color; vec4 material; vec4 view; vec4 emissive; } pc;
const float PI=3.14159265359;
// Scale first so tiny/large derivative normals do not underflow/overflow in dot.
vec3 unit_or(vec3 v,vec3 fallback) {
    if (any(isnan(v)) || any(isinf(v))) {return fallback;}
    float scale=max(max(abs(v.x),abs(v.y)),abs(v.z));
    if (scale<1e-30) {return fallback;}
    vec3 scaled=v/scale;
    return scaled*inversesqrt(dot(scaled,scaled));
}
vec3 perturbed_normal(vec3 n,vec3 texel,vec3 px,vec3 py,vec2 ux,vec2 uy) {
    vec3 t=vec3(0.0);
    vec3 b=vec3(0.0);
    if (abs(world_tangent.w)>0.5) {
        vec3 authored=unit_or(world_tangent.xyz,vec3(0.0));
        vec3 projected=authored-n*dot(n,authored);
        if (dot(projected,projected)>1e-12) {t=unit_or(projected,vec3(0.0));}
        b=cross(n,t)*sign(world_tangent.w);
    }
    if (dot(t,t)<0.5) {
        // Preview fallback, not MikkTSpace. Normalize derivative scales first.
        float us=max(max(abs(ux.x),abs(ux.y)),max(abs(uy.x),abs(uy.y)));
        float ps=max(max(max(abs(px.x),abs(px.y)),abs(px.z)),max(max(abs(py.x),abs(py.y)),abs(py.z)));
        if (isnan(us) || isinf(us) || isnan(ps) || isinf(ps) || us<1e-30 || ps<1e-30) {return n;}
        vec2 dx=ux/us;
        vec2 dy=uy/us;
        float det=dx.x*dy.y-dx.y*dy.x;
        if (abs(det)<1e-8) {return n;}
        vec3 a=px/ps;
        vec3 c=py/ps;
        vec3 raw_t=unit_or((a*dy.y-c*dx.y)*sign(det),vec3(0.0));
        vec3 raw_b=unit_or((c*dx.x-a*dy.x)*sign(det),vec3(0.0));
        vec3 projected=raw_t-n*dot(n,raw_t);
        if (dot(projected,projected)<1e-12) {return n;}
        t=unit_or(projected,vec3(0.0));
        float orientation=dot(cross(n,t),raw_b);
        if (dot(t,t)<0.5 || abs(orientation)<1e-6) {return n;}
        b=cross(n,t)*sign(orientation);
    }
    vec3 ts=texel*2.0-1.0;
    ts.xy*=pc.material.w;
    ts=unit_or(ts,vec3(0.0,0.0,1.0));
    return unit_or(t*ts.x+b*ts.y+n*ts.z,n);
}
// ADR-0040 environment light: the sky gradient as a hemisphere light (an
// approximation, not IBL). Colours are already linear; w of zenith = strength.
vec3 sky_color(float y) {
    if (y>=0.0) {return mix(frame.sky_horizon.rgb,frame.sky_zenith.rgb,pow(clamp(y,0.0,1.0),0.55));}
    return mix(frame.sky_horizon.rgb,frame.sky_ground.rgb,pow(clamp(-y,0.0,1.0),0.35));
}
vec3 environment_light(vec3 base,vec3 n,vec3 v,float metal,float rough,vec3 f0) {
    vec3 above=(frame.sky_zenith.rgb+frame.sky_horizon.rgb)*0.5;
    vec3 below=(frame.sky_ground.rgb+frame.sky_horizon.rgb)*0.5;
    vec3 irradiance=mix(below,above,clamp(n.y*0.5+0.5,0.0,1.0));
    float nv=clamp(dot(n,v),0.0,1.0);
    vec3 fresnel=f0+(max(vec3(1.0-rough),f0)-f0)*pow(1.0-nv,5.0);
    vec3 reflected=mix(sky_color(reflect(-v,n).y),irradiance,rough);
    return ((vec3(1.0)-fresnel)*(1.0-metal)*base*irradiance+fresnel*reflected)*frame.sky_zenith.w;
}
vec3 direct_pbr(vec3 base,vec3 n,vec2 mr,vec3 v,float occlusion,vec3 emission) {
    vec3 l=frame.light_direction.xyz;
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
    vec3 radiance=(diffuse+specular)*nl*frame.lighting.x*frame.light_color.rgb+((1.0-metal)*base+f0)*frame.lighting.y*occlusion;
    if (frame.sky_zenith.w>0.0) {radiance+=environment_light(base,n,v,metal,rough,f0)*occlusion;}
    vec3 exposed=max((radiance+emission)*exp2(frame.eye_exposure.w),vec3(0.0));
    return exposed/(vec3(1.0)+exposed);
}
void main() {
    vec3 px=dFdx(world_position);
    vec3 py=dFdy(world_position);
    vec2 normal_dx=dFdx(normal_uv);
    vec2 normal_dy=dFdy(normal_uv);
    vec3 face=cross(px,py);
    vec3 n=face;
    n/=max(length(n),0.00000001);
    vec3 light=vec3(0.25)+0.75*abs(dot(n,frame.light_direction.xyz))*frame.light_color.rgb;
    vec4 base=pc.base_color*tint;
    uint flags=uint(pc.view.x);
    if ((flags&2u)!=0u) {base*=texture(sampler2D(base_color_image,base_color_sampler),uv);}
    vec3 linear=max(base.rgb*light,vec3(0.0));
    bool raw=false;
    if ((flags&1u)!=0u) {
        vec2 mr=pc.material.xy;
        if ((flags&8u)!=0u) {
            // glTF data texture is linear: B=metal, G=roughness; ignore R/A.
            mr*=texture(sampler2D(metallic_roughness_image,metallic_roughness_sampler),mr_uv).bg;
        }
        vec3 v=unit_or(frame.eye_exposure.xyz-world_position,vec3(0.0,0.0,1.0));
        vec3 surface=unit_or(world_normal,vec3(0.0));
        if (dot(surface,surface)<0.5) {
            // No authored normal: use a view-facing geometric frame for this
            // two-sided preview rather than flipping its UV axes after mapping.
            surface=unit_or(face,vec3(0.0,1.0,0.0));
            if (dot(surface,v)<0.0) {surface=-surface;}
        }
        float side=dot(surface,v)<0.0?-1.0:1.0;
        if ((flags&16u)!=0u) {
            // Sample before per-fragment frame fallback and alpha discard.
            vec3 texel=texture(sampler2D(normal_image,normal_sampler),normal_uv).rgb;
            surface=perturbed_normal(surface,texel,px,py,normal_dx,normal_dy);
        }
        float occlusion=1.0;
        if ((flags&32u)!=0u) {
            occlusion=mix(1.0,texture(sampler2D(occlusion_image,occlusion_sampler),ao_uv).r,pc.view.y);
        }
        vec3 emission=vec3(0.0);
        if ((flags&64u)!=0u) {
            // Hardware SRGB decode of RGB; emission alpha never affects coverage.
            emission=pc.emissive.rgb*texture(sampler2D(emissive_image,emissive_sampler),emissive_uv).rgb;
        }
        linear=direct_pbr(base.rgb,surface*side,mr,v,occlusion,emission);
        // ADR-0042 render channels: data values are shown as they are.
        uint channel=uint(frame.lighting.z+0.5);
        if (channel==1u) {linear=surface*side*0.5+0.5; raw=true;}
        else if (channel==2u) {
            float checker=mod(floor(uv.x*8.0)+floor(uv.y*8.0),2.0);
            linear=vec3(fract(uv),0.0)*(0.7+0.3*checker); raw=true;
        }
        else if (channel==3u) {linear=base.rgb;}
        else if (channel==4u) {linear=vec3(mr.x); raw=true;}
        else if (channel==5u) {linear=vec3(mr.y); raw=true;}
        else if (channel==6u) {linear=vec3(occlusion); raw=true;}
        else if (channel==7u) {linear=emission;}
    }
    if (pc.material.z>=0.0 && base.a<pc.material.z) {discard;}
    if ((flags&128u)!=0u) {color=vec4(1.0,0.72,0.2,1.0); return;}
    vec3 display=linear;
    if (raw) {color=vec4(clamp(display,0.0,1.0),1.0); return;}
    if ((flags&4u)!=0u) {
        display=mix(12.92*linear,1.055*pow(linear,vec3(1.0/2.4))-0.055,greaterThan(linear,vec3(0.0031308)));
    }
    color=vec4(display,1.0);
}
