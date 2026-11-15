#version 450
layout(location=0) in vec3 world_position;
layout(location=0) out vec4 color;
layout(push_constant) uniform Push { mat4 view_projection; vec4 base_color; uint encode_srgb; } pc;
void main() {
    vec3 n=cross(dFdx(world_position),dFdy(world_position));
    n/=max(length(n),0.00000001);
    float light=0.25+0.75*abs(dot(n,normalize(vec3(0.4,0.8,0.3))));
    vec3 linear=max(pc.base_color.rgb*light,vec3(0.0));
    vec3 display=linear;
    if (pc.encode_srgb!=0u) {
        display=mix(12.92*linear,1.055*pow(linear,vec3(1.0/2.4))-0.055,greaterThan(linear,vec3(0.0031308)));
    }
    color=vec4(display,1.0);
}
