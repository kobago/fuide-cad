// Screen-space thick lines: one instance per segment, expanded into a quad in pixel space so
// width is constant on screen. Premultiplied alpha out.

struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec3<f32>,
    time: f32,
    accent: vec3<f32>,
    holo_mix: f32,
    params: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct LineParams {
    alpha: f32,
    width_scale: f32,
    // 1 = extend the ends by half the width so joins overlap (overlay lines);
    // 0 = square ends exactly at the vertices (depth-tested edges: an extension would poke
    // out past silhouette corners where nothing occludes it)
    cap: f32,
    _pad: f32,
};
@group(1) @binding(0) var<uniform> lp: LineParams;

struct Inst {
    @location(0) a: vec3<f32>,
    @location(1) width: f32,
    @location(2) b: vec3<f32>,
    @location(3) depth_bias: f32,
    @location(4) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[vi];
    var ca = g.view_proj * vec4<f32>(inst.a, 1.0);
    var cb = g.view_proj * vec4<f32>(inst.b, 1.0);
    let eps = 1e-3;
    var out: VsOut;
    out.color = vec4<f32>(inst.color.rgb, inst.color.a * lp.alpha);
    if (ca.w < eps && cb.w < eps) {
        out.clip = vec4<f32>(0.0, 0.0, -1.0, 1.0); // off screen
        return out;
    }
    // clip against the near plane so segments crossing it stay straight
    if (ca.w < eps) {
        let t = (eps - ca.w) / (cb.w - ca.w);
        ca = mix(ca, cb, t);
    } else if (cb.w < eps) {
        let t = (eps - cb.w) / (ca.w - cb.w);
        cb = mix(cb, ca, t);
    }
    let half_vp = g.params.zw * 0.5;
    let sa = ca.xy / ca.w * half_vp;
    let sb = cb.xy / cb.w * half_vp;
    var d = sb - sa;
    let len = length(d);
    if (len < 1e-4) {
        d = vec2<f32>(1.0, 0.0);
    } else {
        d = d / len;
    }
    let hw = inst.width * lp.width_scale * 0.5;
    let nrm = vec2<f32>(-d.y, d.x) * hw;
    let at_b = c.x > 0.5;
    let end = select(ca, cb, at_b);
    let s_end = select(sa, sb, at_b);
    // optionally extend by half the width along the segment so joins overlap
    let cap = d * hw * lp.cap * select(-1.0, 1.0, at_b);
    let s = s_end + nrm * c.y + cap;
    let w = end.w;
    out.clip = vec4<f32>(s / half_vp * w, end.z - inst.depth_bias * w, w);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
}
