//! The mesh kernel: Manifold (pure-Rust port) behind plain functions. Units are millimetres.
//!
//! Solids are closed triangle meshes; booleans are exact and never fail on coplanar faces,
//! which is what the B-rep kernel (`kernel`, truck) could not promise. Curved surfaces are
//! polygonal: the segment count comes from a chord tolerance ([`segments`]). Threads and other
//! shapes truck cannot sweep are generated directly as meshes ([`thread`]).

use manifold_rust::linalg::{Vec2, Vec3};
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{Error, MeshGL64, Polygons};
use std::f64::consts::PI;

pub use crate::kernel::{BoolOp, KernelError, Result};

/// Chord tolerance (mm) for curved surfaces in the viewport and exports.
pub const MESH_TOL: f64 = 0.02;

/// Segments around a circle of radius `r` so no chord deviates more than `tol`.
pub fn segments(r: f64, tol: f64) -> i32 {
    if r <= 0.0 {
        return 3;
    }
    let t = (tol / r).clamp(1e-6, 0.5);
    let n = (PI / (1.0 - t).acos()).ceil() as i32;
    n.clamp(8, 512)
}

fn check(what: &str, m: Manifold) -> Result<Manifold> {
    match m.status() {
        Error::NoError => Ok(m),
        e => Err(KernelError(format!("{what}: {e:?}"))),
    }
}

pub fn v3(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

/// Axis-aligned box with its minimum corner at `min`.
pub fn make_box(min: [f64; 3], size: [f64; 3]) -> Result<Manifold> {
    if size.iter().any(|s| *s <= 0.0) {
        return Err(KernelError(format!(
            "box size must be positive: {} x {} x {}",
            size[0], size[1], size[2]
        )));
    }
    check("box", Manifold::cube(v3(size), false).translate(v3(min)))
}

/// Cylinder from `base` along `axis` (x, y or z unit direction; any sign).
pub fn make_cylinder(
    base: [f64; 3],
    axis: [f64; 3],
    radius: f64,
    height: f64,
    tol: f64,
) -> Result<Manifold> {
    if radius <= 0.0 || height <= 0.0 {
        return Err(KernelError(format!(
            "cylinder radius / height must be positive: r={radius} h={height}"
        )));
    }
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len < 1e-12 {
        return Err(KernelError("cylinder axis is zero".into()));
    }
    let a = [axis[0] / len, axis[1] / len, axis[2] / len];
    let cyl = Manifold::cylinder(height, radius, radius, segments(radius, tol));
    // rotate +Z onto the axis: rotate about Y by the polar angle, then about Z by the azimuth
    let polar = a[2].clamp(-1.0, 1.0).acos().to_degrees();
    let azimuth = a[1].atan2(a[0]).to_degrees();
    let cyl = if polar.abs() < 1e-9 {
        cyl
    } else {
        cyl.rotate(0.0, polar, 0.0).rotate(0.0, 0.0, azimuth)
    };
    check("cylinder", cyl.translate(v3(base)))
}

/// `a op b`. Manifold booleans do not fail; an empty result is still a valid (empty) solid.
pub fn boolean(op: BoolOp, a: &Manifold, b: &Manifold) -> Result<Manifold> {
    let m = match op {
        BoolOp::Union => a.union(b),
        BoolOp::Difference => a.difference(b),
        BoolOp::Intersection => a.intersection(b),
    };
    check(op.label(), m)
}

pub fn translate(m: &Manifold, by: [f64; 3]) -> Manifold {
    m.translate(v3(by))
}

/// Rotate about the axis through `origin` (degrees).
pub fn rotate(m: &Manifold, origin: [f64; 3], axis: [f64; 3], degrees: f64) -> Result<Manifold> {
    let len = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if len < 1e-12 {
        return Err(KernelError("rotation axis is zero".into()));
    }
    let a = [axis[0] / len, axis[1] / len, axis[2] / len];
    let o = v3(origin);
    let moved = m.translate(Vec3::new(-o.x, -o.y, -o.z));
    // Manifold rotates about X, then Y, then Z: express an arbitrary axis by aligning it to Z
    let polar = a[2].clamp(-1.0, 1.0).acos().to_degrees();
    let azimuth = a[1].atan2(a[0]).to_degrees();
    let turned = if polar.abs() < 1e-9 {
        moved.rotate(0.0, 0.0, degrees)
    } else if (polar - 180.0).abs() < 1e-9 {
        moved.rotate(0.0, 0.0, -degrees)
    } else {
        // to axis frame: undo azimuth, undo polar; spin about Z; redo
        moved
            .rotate(0.0, 0.0, -azimuth)
            .rotate(0.0, -polar, 0.0)
            .rotate(0.0, 0.0, degrees)
            .rotate(0.0, polar, 0.0)
            .rotate(0.0, 0.0, azimuth)
    };
    check("rotate", turned.translate(o))
}

/// An ISO-style external thread as a closed mesh: major diameter `d`, pitch `p`, along +Z
/// from z = 0 to `length`, 60° flanks with a flat crest (1/8 p) and root (1/4 p).
///
/// The surface is a grid over (angle, phase): each grid row is a helix at a fixed point of
/// the profile, so the crest / flank / root boundaries are mesh edges (crisp crease lines,
/// clean normals) and a flank needs a single quad strip. The helical grid overhangs both
/// ends by a pitch and is trimmed flat by two planes.
pub fn thread(d: f64, p: f64, length: f64, tol: f64) -> Result<Manifold> {
    if d <= 0.0 || p <= 0.0 || length <= 0.0 {
        return Err(KernelError(format!(
            "thread needs positive diameter / pitch / length: d={d} p={p} l={length}"
        )));
    }
    let r_major = d / 2.0;
    let depth = 0.625 * 0.866_025 * p;
    if depth >= r_major {
        return Err(KernelError("thread depth exceeds the radius".into()));
    }
    let r_minor = r_major - depth;
    let n = segments(r_major, tol).max(48) as usize; // around
                                                     // profile breakpoints (phase u in [0, 1)): crest start, crest end, root start, root end
    let (crest, root) = (0.125, 0.25);
    let flank = (1.0 - crest - root) / 2.0;
    let breaks: [(f64, f64); 4] = [
        (0.0, r_major),
        (crest, r_major),
        (crest + flank, r_minor),
        (crest + flank + root, r_minor),
    ];
    let turns = (length / p).ceil() as usize + 2; // one spare pitch at each end
    let rows = turns * breaks.len() + 1;
    let mut pos: Vec<f64> = Vec::with_capacity((rows * n + 2) * 3);
    for j in 0..rows {
        let (u, r) = breaks[j % breaks.len()];
        let turn = (j / breaks.len()) as f64;
        for i in 0..n {
            let th = i as f64 / n as f64 * 2.0 * PI;
            // z of this helix row at angle th, shifted down by a pitch so z = 0 is inside
            let z = p * (turn + u + th / (2.0 * PI)) - p;
            pos.extend_from_slice(&[r * th.cos(), r * th.sin(), z]);
        }
    }
    // cap the helical ends with fans (they get trimmed away, but the mesh must be closed):
    // the bottom row (j = 0) and the top row (j = rows - 1) are helices, so close each with
    // one extra vertex on the axis and a "seam" quad between the row ends
    let bottom = (rows * n) as u64;
    let top = bottom + 1;
    pos.extend_from_slice(&[0.0, 0.0, -1.5 * p, 0.0, 0.0, p * (turns as f64 + 0.5)]);
    let idx = |i: usize, j: usize| -> u64 {
        // wrapping around the seam advances one row (the helix climbs one pitch per turn)
        if i < n {
            (j * n + i) as u64
        } else {
            ((j + breaks.len()) * n) as u64 + (i - n) as u64
        }
    };
    let k = breaks.len();
    // quads between consecutive rows; the seam (i = n) reaches row j + k, so the top quad
    // row must leave k rows above it
    let mut tri: Vec<u64> = Vec::with_capacity((rows * n * 2 + 4 * n) * 3);
    for j in 0..rows - k - 1 {
        for i in 0..n {
            let (a, b, c, dd) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            tri.extend_from_slice(&[a, b, c, a, c, dd]);
        }
    }
    // The open boundary at the bottom runs once around row 0, arrives at (0, k) through the
    // seam and comes back down the seam's vertical edges (0, k-1) .. (0, 1). Fan it to the
    // bottom apex (normal -Z); the top boundary is the mirror image at row T.
    let mut bottom_ring: Vec<u64> = (0..n).map(|i| idx(i, 0)).collect();
    bottom_ring.extend((1..=k).rev().map(|j| idx(0, j)));
    for w in 0..bottom_ring.len() {
        let (a, b) = (bottom_ring[w], bottom_ring[(w + 1) % bottom_ring.len()]);
        tri.extend_from_slice(&[bottom, b, a]);
    }
    let t_row = rows - k - 1;
    let mut top_ring: Vec<u64> = (0..n).map(|i| idx(i, t_row)).collect();
    top_ring.extend((t_row + 1..=t_row + k).rev().map(|j| idx(0, j)));
    for w in 0..top_ring.len() {
        let (a, b) = (top_ring[w], top_ring[(w + 1) % top_ring.len()]);
        tri.extend_from_slice(&[top, a, b]);
    }
    let mesh = MeshGL64 {
        num_prop: 3,
        vert_properties: pos,
        tri_verts: tri,
        ..Default::default()
    };
    let raw = check("thread", Manifold::from_mesh_gl64(&mesh))?;
    // flat ends: keep z >= 0, then z <= length
    let cut = raw
        .trim_by_plane(Vec3::new(0.0, 0.0, 1.0), 0.0)
        .trim_by_plane(Vec3::new(0.0, 0.0, -1.0), -length);
    check("thread trim", cut)
}

/// Extrude a simple polygon (CCW, in the XY plane) from z = 0 to `height`.
pub fn extrude(polygon: &[[f64; 2]], height: f64) -> Result<Manifold> {
    if polygon.len() < 3 {
        return Err(KernelError("extrude needs at least 3 points".into()));
    }
    if height <= 0.0 {
        return Err(KernelError(format!(
            "extrude height must be positive: {height}"
        )));
    }
    let poly: Polygons = vec![polygon.iter().map(|p| Vec2::new(p[0], p[1])).collect()];
    check(
        "extrude",
        Manifold::extrude(&poly, height, 0, 0.0, Vec2::new(1.0, 1.0)),
    )
}

/// Circular backlash as a fraction of the module: the tooth is thinned by this so a pair at
/// the standard centre distance turns freely instead of touching everywhere.
pub const GEAR_BACKLASH: f64 = 0.05;

/// Involute spur gear outline (CCW, centred on the origin, one tooth centred on +X):
/// module `m`, `z` teeth, pressure angle `alpha_deg`. Addendum = m, dedendum = 1.25 m,
/// backlash [`GEAR_BACKLASH`] · m. `tol` sets the flank resolution.
pub fn gear_profile(m: f64, z: u32, alpha_deg: f64, tol: f64) -> Result<Vec<[f64; 2]>> {
    if m <= 0.0 || z < 4 || !(5.0..=35.0).contains(&alpha_deg) {
        return Err(KernelError(format!(
            "gear needs module > 0, at least 4 teeth and a pressure angle of 5..35°: m={m} z={z} alpha={alpha_deg}"
        )));
    }
    let alpha = alpha_deg.to_radians();
    let zf = z as f64;
    let rp = m * zf / 2.0; // pitch
    let rb = rp * alpha.cos(); // base
    let ra = rp + m; // tip
    let rf = rp - 1.25 * m; // root
    if rf <= 0.0 {
        return Err(KernelError("gear root radius is not positive".into()));
    }
    let inv = |a: f64| a.tan() - a;
    // half tooth angle at radius r (r >= rb)
    let half_tooth = PI / (2.0 * zf) - GEAR_BACKLASH * m / (2.0 * rp);
    let psi = |r: f64| half_tooth + inv(alpha) - inv((rb / r).clamp(-1.0, 1.0).acos());
    let r0 = rb.max(rf);
    let flank_pts = ((ra - r0) / (2.0 * tol).max(1e-3)).ceil().clamp(4.0, 24.0) as usize;
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(z as usize * (2 * flank_pts + 8));
    let pt = |r: f64, a: f64| [r * a.cos(), r * a.sin()];
    let half_gap = PI / zf; // angle from a tooth centre to the middle of the gap
    for k in 0..z {
        let c = 2.0 * PI * k as f64 / zf;
        // root arc from the gap middle to the start of the right flank
        let a0 = c - half_gap;
        let a1 = c - psi(r0);
        for i in 0..3 {
            let a = a0 + (a1 - a0) * i as f64 / 3.0;
            out.push(pt(rf, a));
        }
        if rf < rb {
            out.push(pt(rf, a1));
        }
        // flank up (right side, negative angle)
        for i in 0..=flank_pts {
            let r = r0 + (ra - r0) * i as f64 / flank_pts as f64;
            out.push(pt(r, c - psi(r)));
        }
        // tip arc (the middle point puts a vertex exactly on the tip circle)
        let ta = psi(ra);
        out.push(pt(ra, c - ta / 2.0));
        out.push(pt(ra, c));
        out.push(pt(ra, c + ta / 2.0));
        // flank down (left side)
        for i in (0..=flank_pts).rev() {
            let r = r0 + (ra - r0) * i as f64 / flank_pts as f64;
            out.push(pt(r, c + psi(r)));
        }
        if rf < rb {
            out.push(pt(rf, c + psi(r0)));
        }
        // root arc to the next gap middle
        let b0 = c + psi(r0);
        let b1 = c + half_gap;
        for i in 1..3 {
            let a = b0 + (b1 - b0) * i as f64 / 3.0;
            out.push(pt(rf, a));
        }
    }
    Ok(out)
}

/// A spur gear standing on z = 0, axis +Z, `width` thick.
pub fn gear(m: f64, z: u32, alpha_deg: f64, width: f64, tol: f64) -> Result<Manifold> {
    let profile = gear_profile(m, z, alpha_deg, tol)?;
    extrude(&profile, width)
}

/// Flat per-triangle vertex arrays plus feature edges (dihedral angle over `crease_deg`).
#[derive(Debug, Clone, Default)]
pub struct MeshOut {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub edges: Vec<[[f32; 3]; 2]>,
    pub tri_count: usize,
}

pub fn mesh_out(m: &Manifold, crease_deg: f64) -> MeshOut {
    let gl = m.get_mesh_gl64(-1);
    let nt = gl.num_tri();
    let nv = gl.num_vert();
    let vert = |v: u64| -> [f64; 3] { gl.get_vert_pos(v as usize) };
    let cos_crease = crease_deg.to_radians().cos();
    // per-triangle normals and the triangles around each vertex
    let mut tri_normals = Vec::with_capacity(nt);
    let mut around: Vec<Vec<usize>> = vec![Vec::new(); nv];
    for t in 0..nt {
        let [a, b, c] = gl.get_tri_verts(t);
        let (pa, pb, pc) = (vert(a), vert(b), vert(c));
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let w = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n = [
            u[1] * w[2] - u[2] * w[1],
            u[2] * w[0] - u[0] * w[2],
            u[0] * w[1] - u[1] * w[0],
        ];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-30);
        tri_normals.push([n[0] / l, n[1] / l, n[2] / l]);
        for v in [a, b, c] {
            around[v as usize].push(t);
        }
    }
    // smooth normals: at each corner, average the normals of the surrounding triangles that
    // face within the crease angle of this one (cylinders and flanks go smooth, box corners
    // and tooth tips stay sharp)
    let mut positions = Vec::with_capacity(nt * 3);
    let mut normals = Vec::with_capacity(nt * 3);
    for t in 0..nt {
        let n0 = tri_normals[t];
        for v in gl.get_tri_verts(t) {
            let p = vert(v);
            positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
            let mut acc = [0.0f64; 3];
            for &o in &around[v as usize] {
                let n1 = tri_normals[o];
                if n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2] >= cos_crease {
                    acc[0] += n1[0];
                    acc[1] += n1[1];
                    acc[2] += n1[2];
                }
            }
            let l = (acc[0] * acc[0] + acc[1] * acc[1] + acc[2] * acc[2]).sqrt();
            let n = if l > 1e-12 {
                [acc[0] / l, acc[1] / l, acc[2] / l]
            } else {
                n0
            };
            normals.push([n[0] as f32, n[1] as f32, n[2] as f32]);
        }
    }
    // feature edges: each undirected edge is shared by two triangles in a manifold mesh
    let mut owner: std::collections::HashMap<(u64, u64), usize> = std::collections::HashMap::new();
    let mut edges = Vec::new();
    for t in 0..nt {
        let tv = gl.get_tri_verts(t);
        for k in 0..3 {
            let (a, b) = (tv[k], tv[(k + 1) % 3]);
            let key = (a.min(b), a.max(b));
            match owner.get(&key) {
                None => {
                    owner.insert(key, t);
                }
                Some(&o) => {
                    let (n0, n1) = (tri_normals[o], tri_normals[t]);
                    let dot = n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2];
                    if dot < cos_crease {
                        let (pa, pb) = (vert(a), vert(b));
                        edges.push([
                            [pa[0] as f32, pa[1] as f32, pa[2] as f32],
                            [pb[0] as f32, pb[1] as f32, pb[2] as f32],
                        ]);
                    }
                }
            }
        }
    }
    let indices = (0..positions.len() as u32).collect();
    MeshOut {
        positions,
        normals,
        indices,
        edges,
        tri_count: nt,
    }
}

/// Volume (mm³) and centroid via the divergence theorem over the triangles.
pub fn volume_and_centroid(m: &Manifold) -> (f64, [f64; 3]) {
    let gl = m.get_mesh_gl64(-1);
    let mut vol = 0.0;
    let mut c = [0.0; 3];
    for t in 0..gl.num_tri() {
        let [a, b, d] = gl.get_tri_verts(t);
        let (p, q, r) = (
            gl.get_vert_pos(a as usize),
            gl.get_vert_pos(b as usize),
            gl.get_vert_pos(d as usize),
        );
        let det = p[0] * (q[1] * r[2] - q[2] * r[1]) - p[1] * (q[0] * r[2] - q[2] * r[0])
            + p[2] * (q[0] * r[1] - q[1] * r[0]);
        vol += det / 6.0;
        for k in 0..3 {
            c[k] += det / 24.0 * (p[k] + q[k] + r[k]);
        }
    }
    if vol.abs() > 1e-12 {
        for v in &mut c {
            *v /= vol;
        }
    }
    (vol, c)
}

pub fn bbox(m: &Manifold) -> ([f64; 3], [f64; 3]) {
    let b = m.bounding_box();
    ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z])
}

/// Binary STL of one or more meshes (their triangles concatenated).
pub fn write_stl<W: std::io::Write>(meshes: &[&MeshOut], w: &mut W) -> std::io::Result<usize> {
    let n: usize = meshes.iter().map(|m| m.tri_count).sum();
    w.write_all(&[0u8; 80])?;
    w.write_all(&(n as u32).to_le_bytes())?;
    for m in meshes {
        for t in 0..m.tri_count {
            let nrm = m.normals[t * 3];
            for v in nrm {
                w.write_all(&v.to_le_bytes())?;
            }
            for k in 0..3 {
                for v in m.positions[t * 3 + k] {
                    w.write_all(&v.to_le_bytes())?;
                }
            }
            w.write_all(&[0u8; 2])?;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1.0)
    }

    #[test]
    fn box_minus_cylinder_through_hole() {
        let b = make_box([0.0; 3], [40.0, 40.0, 10.0]).unwrap();
        let hole = make_cylinder([20.0, 20.0, -5.0], [0.0, 0.0, 1.0], 5.0, 20.0, MESH_TOL).unwrap();
        let cut = boolean(BoolOp::Difference, &b, &hole).unwrap();
        let (v, c) = volume_and_centroid(&cut);
        assert!(near(v, 16_000.0 - PI * 25.0 * 10.0, 3e-3), "v={v}");
        assert!(near(c[0], 20.0, 1e-6) && near(c[2], 5.0, 1e-6), "{c:?}");
        let out = mesh_out(&cut, 30.0);
        assert!(out.tri_count > 12);
        // 4 vertical box edges + 8 horizontal + the two hole rims: no crease edges on the hole wall
        let vertical = out
            .edges
            .iter()
            .filter(|e| (e[0][0] - e[1][0]).abs() < 1e-6 && (e[0][1] - e[1][1]).abs() < 1e-6)
            .count();
        assert_eq!(vertical, 4, "{}", out.edges.len());
        let mut buf = Vec::new();
        let n = write_stl(&[&out], &mut buf).unwrap();
        assert_eq!(buf.len(), 84 + 50 * n);
    }

    #[test]
    fn coplanar_union_and_cut_are_exact() {
        let a = make_box([0.0; 3], [60.0, 40.0, 6.0]).unwrap();
        let b = make_box([0.0; 3], [6.0, 40.0, 30.0]).unwrap();
        let (v, _) = volume_and_centroid(&boolean(BoolOp::Union, &a, &b).unwrap());
        assert!(near(v, 14_400.0 + 6.0 * 40.0 * 24.0, 1e-9), "v={v}");
        let (v, _) = volume_and_centroid(&boolean(BoolOp::Difference, &a, &b).unwrap());
        assert!(near(v, 14_400.0 - 6.0 * 40.0 * 6.0, 1e-9), "v={v}");
        let (v, _) = volume_and_centroid(&boolean(BoolOp::Intersection, &a, &b).unwrap());
        assert!(near(v, 6.0 * 40.0 * 6.0, 1e-9), "v={v}");
    }

    #[test]
    fn cylinder_along_any_axis_and_rotation() {
        let cx = make_cylinder([0.0; 3], [1.0, 0.0, 0.0], 5.0, 20.0, MESH_TOL).unwrap();
        let (lo, hi) = bbox(&cx);
        assert!(
            near(hi[0], 20.0, 1e-9) && near(lo[1], -5.0, 1e-3) && near(hi[2], 5.0, 1e-3),
            "{lo:?} {hi:?}"
        );
        let cy = make_cylinder([1.0, 2.0, 3.0], [0.0, -1.0, 0.0], 5.0, 20.0, MESH_TOL).unwrap();
        let (lo, hi) = bbox(&cy);
        assert!(
            near(lo[1], -18.0, 1e-9) && near(hi[1], 2.0, 1e-9),
            "{lo:?} {hi:?}"
        );
        let b = make_box([0.0; 3], [10.0, 20.0, 30.0]).unwrap();
        let (_, c) = volume_and_centroid(&rotate(&b, [0.0; 3], [0.0, 0.0, 1.0], 90.0).unwrap());
        assert!(near(c[0], -10.0, 1e-9) && near(c[1], 5.0, 1e-9), "{c:?}");
        let (_, c) = volume_and_centroid(&rotate(&b, [0.0; 3], [1.0, 0.0, 0.0], 90.0).unwrap());
        assert!(near(c[1], -15.0, 1e-9) && near(c[2], 10.0, 1e-9), "{c:?}");
    }

    #[test]
    fn hex_bolt_with_a_real_thread() {
        let (af, head, d, p, len) = (13.0, 6.0, 8.0, 1.25, 30.0);
        let blank = || make_box([-af / 2.0, -30.0, 0.0], [af, 60.0, head]).unwrap();
        let b = rotate(&blank(), [0.0; 3], [0.0, 0.0, 1.0], 60.0).unwrap();
        let c = rotate(&blank(), [0.0; 3], [0.0, 0.0, 1.0], 120.0).unwrap();
        let hex = boolean(
            BoolOp::Intersection,
            &boolean(BoolOp::Intersection, &blank(), &b).unwrap(),
            &c,
        )
        .unwrap();
        let (v, _) = volume_and_centroid(&hex);
        assert!(
            near(v, 3f64.sqrt() / 2.0 * af * af * head, 1e-6),
            "hex v={v}"
        );
        let th = thread(d, p, len, MESH_TOL).unwrap();
        assert_eq!(th.status(), Error::NoError);
        assert_eq!(th.genus(), 0);
        let (lo, hi) = bbox(&th);
        assert!(
            near(lo[2], 0.0, 1e-9) && near(hi[2], len, 1e-9),
            "{lo:?} {hi:?}"
        );
        let (tv, _) = volume_and_centroid(&th);
        let (r_maj, r_min) = (d / 2.0, d / 2.0 - 0.625 * 0.866_025 * p);
        assert!(
            tv > PI * r_min * r_min * len && tv < PI * r_maj * r_maj * len,
            "thread v={tv}"
        );
        let bolt = boolean(
            BoolOp::Union,
            &hex,
            &translate(&th, [0.0, 0.0, head - 0.5]).translate(v3([0.0; 3])),
        )
        .unwrap();
        let (bv, _) = volume_and_centroid(&bolt);
        assert!(
            near(bv, v + tv - PI * r_min * r_min * 0.5 * 1.1, 2e-2),
            "bolt v={bv} hex {v} thread {tv}"
        );
        let out = mesh_out(&bolt, 30.0);
        assert!(out.tri_count > 3_000, "{}", out.tri_count);
    }

    #[test]
    fn involute_gear_pair() {
        let (m, z1, z2) = (1.5, 12u32, 36u32);
        let p = gear_profile(m, z1, 20.0, MESH_TOL).unwrap();
        let r: Vec<f64> = p
            .iter()
            .map(|q| (q[0] * q[0] + q[1] * q[1]).sqrt())
            .collect();
        let (rmax, rmin) = (
            r.iter().cloned().fold(0.0, f64::max),
            r.iter().cloned().fold(1e9, f64::min),
        );
        assert!(near(rmax, m * z1 as f64 / 2.0 + m, 1e-9), "tip {rmax}");
        assert!(
            near(rmin, m * z1 as f64 / 2.0 - 1.25 * m, 1e-9),
            "root {rmin}"
        );
        // CCW: positive signed area, between the root and tip circles
        let area: f64 = p
            .iter()
            .zip(p.iter().cycle().skip(1))
            .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
            .sum::<f64>()
            / 2.0;
        assert!(
            area > PI * rmin * rmin && area < PI * rmax * rmax,
            "area {area}"
        );
        let g1 = gear(m, z1, 20.0, 8.0, MESH_TOL).unwrap();
        let (v, _) = volume_and_centroid(&g1);
        assert!(near(v, area * 8.0, 1e-6), "v={v}");
        // a wheel meshing with the pinion at the standard centre distance does not overlap it
        let g2 = gear(m, z2, 20.0, 8.0, MESH_TOL).unwrap();
        let cd = m * (z1 + z2) as f64 / 2.0;
        let g2 = rotate(&g2, [0.0; 3], [0.0, 0.0, 1.0], 180.0 / z2 as f64).unwrap();
        let g2 = translate(&g2, [cd, 0.0, 0.0]);
        let (overlap, _) = volume_and_centroid(&boolean(BoolOp::Intersection, &g1, &g2).unwrap());
        assert!(overlap < 1e-6, "gears collide: {overlap}");
        // and they do touch: moving the wheel 0.3 mm closer makes them intersect
        let g2c = translate(&g2, [-0.3, 0.0, 0.0]);
        let (overlap, _) = volume_and_centroid(&boolean(BoolOp::Intersection, &g1, &g2c).unwrap());
        assert!(overlap > 0.0, "gears do not mesh");
    }

    #[test]
    fn thread_edges_are_helical_crease_lines() {
        let th = thread(8.0, 1.25, 10.0, MESH_TOL).unwrap();
        let out = mesh_out(&th, 30.0);
        // crest and root boundaries: 4 helices over 8 turns, each about `segments` long,
        // plus the two end outlines — and nothing on the flanks themselves
        let helical = out
            .edges
            .iter()
            .filter(|e| (e[0][2] - e[1][2]).abs() > 1e-4 && e[0][2] > 0.01 && e[1][2] < 9.99)
            .count();
        assert!(helical > 4 * 8 * 40, "{helical} helical edge segments");
        assert!(out.edges.len() < helical * 2, "{} edges", out.edges.len());
    }

    #[test]
    fn smooth_normals_on_a_cylinder_sharp_on_a_box() {
        let c = make_cylinder([0.0; 3], [0.0, 0.0, 1.0], 5.0, 10.0, MESH_TOL).unwrap();
        let out = mesh_out(&c, 30.0);
        // side vertices: the normal points radially through the vertex (smooth), not along a facet
        let mut checked = 0;
        for (p, n) in out.positions.iter().zip(out.normals.iter()) {
            if n[2].abs() < 1e-3 {
                let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
                let dot = (p[0] * n[0] + p[1] * n[1]) / r;
                assert!(dot > 0.999, "corner normal not radial: {p:?} {n:?}");
                checked += 1;
            }
        }
        assert!(checked > 0);
        let b = make_box([0.0; 3], [10.0; 3]).unwrap();
        let out = mesh_out(&b, 30.0);
        for n in &out.normals {
            assert!(
                n.iter().any(|v| v.abs() > 0.999),
                "box normal smeared: {n:?}"
            );
        }
    }

    #[test]
    fn segments_from_tolerance() {
        assert_eq!(segments(5.0, 0.02), 36);
        assert!(segments(0.5, 0.02) >= 8 && segments(100.0, 0.02) <= 512);
    }
}
