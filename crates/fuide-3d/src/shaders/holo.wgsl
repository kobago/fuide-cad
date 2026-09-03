// Hologram fill: flat shading from the vertex normal, fresnel rim glow in the accent colour,
// scan lines along world Z. Premultiplied alpha out.

struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    time: f32,
    accent: vec3<f32>,
    holo_mix: f32,
    // scan_strength, fill_alpha, viewport w, viewport h
    params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct Object {
    color: vec4<f32>,
};
@group(1) @binding(0) var<uniform> obj: Object;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) nor: vec3<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nor: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = g.view_proj * vec4<f32>(in.pos, 1.0);
    out.world = in.pos;
    out.nor = in.nor;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.nor);
    let v = normalize(g.cam_pos - in.world);
    let ndv = abs(dot(n, v));
    let fresnel = pow(1.0 - ndv, 2.0);
    let l = normalize(vec3<f32>(0.4, -0.5, 0.8));
    let lambert = 0.35 + 0.65 * abs(dot(n, l));
    let lum = dot(obj.color.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let base = mix(obj.color.rgb, g.accent * (0.25 + 0.75 * lum), g.holo_mix);
    // 220 cycles per metre = 0.22 per mm
    let scan = 1.0 - g.params.x * (0.5 + 0.5 * sin(in.world.z * 0.22 - g.time * 6.0));
    var rgb = base * 0.38 * lambert + g.accent * fresnel * 0.9;
    rgb = rgb * scan + g.accent * 0.02;
    let a = obj.color.a * g.params.y;
    return vec4<f32>(rgb * a, a);
}
