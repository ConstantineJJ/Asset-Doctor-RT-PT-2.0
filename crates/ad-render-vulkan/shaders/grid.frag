#version 450
// ADR-0040: procedural floor themes. style.z selects the pattern: 0 grid,
// 1 matte, 2 light, 3 checker, 4 tiles, 5 noise, 6 water, 7 bricks,
// 8 stones, 9 fields; color_a/color_b come from the theme table in grid.rs.
// Red X axis (z = 0) and blue Z axis (x = 0) on every theme; detail that is
// smaller than a pixel fades to its average so distant floors do not shimmer.
layout(location=0) in vec2 world_xz;
layout(location=0) out vec4 color;
layout(push_constant) uniform Push { mat4 view_projection; vec4 plane; vec4 style; vec4 color_a; vec4 color_b; } pc;

float lines(vec2 p, float step) {
    vec2 c=p/step;
    vec2 d=max(fwidth(c),vec2(0.000001));
    vec2 g=abs(fract(c-0.5)-0.5)/d;
    return 1.0-min(min(g.x,g.y),1.0);
}
float hash(vec2 p) {
    p=fract(p*vec2(123.34,456.21));
    p+=dot(p,p+45.32);
    return fract(p.x*p.y);
}
vec2 hash2(vec2 p) {
    return vec2(hash(p),hash(p+vec2(17.13,3.71)));
}
float noise(vec2 p) {
    vec2 i=floor(p);
    vec2 f=fract(p);
    vec2 u=f*f*(3.0-2.0*f);
    float a=hash(i);
    float b=hash(i+vec2(1.0,0.0));
    float c=hash(i+vec2(0.0,1.0));
    float d=hash(i+vec2(1.0,1.0));
    return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);
}
// Three octaves; each octave fades out once it is smaller than a pixel.
float fbm(vec2 p) {
    float w=length(fwidth(p));
    float sum=0.0;
    float amp=0.5;
    float freq=1.0;
    for (int i=0;i<3;i++) {
        float detail=1.0-smoothstep(0.3,0.8,w*freq);
        sum+=amp*mix(0.5,noise(p*freq),detail);
        freq*=2.7;
        amp*=0.55;
    }
    return sum/0.8775;
}
// Distance to the nearest and second-nearest jittered cell centre.
vec2 cells(vec2 p) {
    vec2 i=floor(p);
    vec2 f=fract(p);
    float d1=8.0;
    float d2=8.0;
    for (int y=-1;y<=1;y++) {
        for (int x=-1;x<=1;x++) {
            vec2 o=vec2(float(x),float(y));
            vec2 r=o+0.15+0.7*hash2(i+o)-f;
            float d=dot(r,r);
            if (d<d1) { d2=d1; d1=d; } else if (d<d2) { d2=d; }
        }
    }
    return vec2(sqrt(d1),sqrt(d2));
}
void main() {
    float s=pc.style.x;
    vec3 a=pc.color_a.rgb;
    vec3 b=pc.color_b.rgb;
    float minor=lines(world_xz,s);
    float major=lines(world_xz,s*pc.style.y);
    vec2 d=max(fwidth(world_xz),vec2(0.000001));
    float x_axis=1.0-min(abs(world_xz.y)/(1.5*d.y),1.0);
    float z_axis=1.0-min(abs(world_xz.x)/(1.5*d.x),1.0);
    int pattern=int(pc.style.z+0.5);
    vec3 rgb;
    float alpha=0.96;
    if (pattern==1) {
        rgb=mix(a,b,major*0.6);
        alpha=0.94;
    } else if (pattern==2) {
        rgb=mix(a,b,minor*0.6);
    } else if (pattern==3) {
        vec2 cell=floor(world_xz/s);
        float parity=mod(cell.x+cell.y,2.0);
        float blur=smoothstep(0.4,1.0,length(fwidth(world_xz/s)));
        rgb=mix(mix(a,b,parity),(a+b)*0.5,blur);
        alpha=0.95;
    } else if (pattern==4) {
        rgb=mix(a,b,lines(world_xz,s*2.0));
    } else if (pattern==5) {
        float n=fbm(world_xz/(s*0.6));
        float fine=fbm(world_xz/(s*0.12)+7.0);
        rgb=mix(a,b,clamp(n*0.75+fine*0.45-0.1,0.0,1.0));
    } else if (pattern==6) {
        vec2 p=world_xz/s;
        float n=fbm(p*0.8);
        float waves=0.5+0.5*sin((p.x*1.7+p.y*0.9)*3.0+n*6.0);
        float blur=smoothstep(0.3,0.9,length(fwidth(p))*3.0);
        rgb=mix(a,b,mix(waves*0.55+n*0.35,0.45,blur));
        alpha=0.9;
    } else if (pattern==7) {
        vec2 q=world_xz/vec2(s*0.5,s*0.25);
        q.x+=0.5*mod(floor(q.y),2.0);
        vec2 f=abs(fract(q)-0.5);
        vec2 w=fwidth(q);
        float mortar=1.0-max(smoothstep(0.46,0.46+w.x,f.x),smoothstep(0.42,0.42+w.y,f.y));
        vec3 brick=a*(0.85+0.3*hash(floor(q)));
        float blur=smoothstep(0.3,0.9,length(w));
        rgb=mix(mix(brick,b,1.0-mortar),mix(a,b,0.25),blur);
    } else if (pattern==8) {
        vec2 p=world_xz/(s*0.3);
        vec2 c=cells(p);
        float edge=1.0-smoothstep(0.03,0.12+length(fwidth(p)),c.y-c.x);
        vec3 stone=a*(0.8+0.4*hash(floor(p*0.999)));
        float blur=smoothstep(0.3,0.9,length(fwidth(p)));
        rgb=mix(mix(stone,b,edge),mix(a,b,0.3),blur);
    } else if (pattern==9) {
        vec2 p=world_xz/s;
        float field=hash(floor(p/vec2(3.0,2.0)));
        vec3 base=mix(a,b,step(0.5,field));
        float furrows=0.5+0.5*sin(p.x*12.0);
        float blur=smoothstep(0.2,0.6,length(fwidth(p))*12.0);
        rgb=base*(0.85+0.15*mix(furrows,0.5,blur))*(0.9+0.2*fbm(p));
    } else {
        rgb=a;
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
