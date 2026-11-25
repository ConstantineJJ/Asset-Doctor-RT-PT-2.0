#version 450
// Outline over the scene: pixels just outside the mask. Selected (red mask)
// is a solid line, hovered (green mask) a dashed one. In the dynamic mode the
// pipeline inverts what is below (output white); otherwise the push colours
// are blended over it.
layout(location=0) in vec2 ndc;
layout(location=0) out vec4 color;
layout(set=0,binding=0) uniform texture2D mask_image;
layout(set=0,binding=1) uniform sampler mask_sampler;
layout(push_constant) uniform Push { vec4 selected; vec4 hovered; vec4 params; } pc;
void main() {
    ivec2 size=textureSize(sampler2D(mask_image,mask_sampler),0);
    ivec2 p=ivec2(gl_FragCoord.xy);
    vec2 here=texelFetch(sampler2D(mask_image,mask_sampler),p,0).rg;
    int r=int(pc.params.x+0.5);
    vec2 near=vec2(0.0);
    // Every pixel within 3 px (thin parts keep their outline), then a
    // sparser grid up to the full width: at most ~17 x 17 samples for any
    // width, so a thick outline stays cheap.
    int core=min(r,3);
    for (int dy=-core; dy<=core; dy++) {
        for (int dx=-core; dx<=core; dx++) {
            if (dx*dx+dy*dy>core*core) {continue;}
            ivec2 q=clamp(p+ivec2(dx,dy),ivec2(0),size-1);
            near=max(near,texelFetch(sampler2D(mask_image,mask_sampler),q,0).rg);
        }
    }
    if (r>core) {
        int step=max(1,(r+7)/8);
        for (int dy=-r; dy<=r; dy+=step) {
            for (int dx=-r; dx<=r; dx+=step) {
                if (dx*dx+dy*dy>r*r) {continue;}
                ivec2 q=clamp(p+ivec2(dx,dy),ivec2(0),size-1);
                near=max(near,texelFetch(sampler2D(mask_image,mask_sampler),q,0).rg);
            }
        }
    }
    bool selected=near.r>0.5 && here.r<0.5;
    bool hovered=near.g>0.5 && here.g<0.5 && here.r<0.5
        && mod(floor((gl_FragCoord.x+gl_FragCoord.y)/pc.params.y),2.0)<0.5;
    if (!selected && !hovered) {discard;}
    if (pc.params.z>0.5) {color=vec4(1.0); return;}
    color=selected?pc.selected:pc.hovered;
}
