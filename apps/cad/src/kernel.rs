//! The truck kernel behind plain functions. Units are millimetres, coordinates `f64`.
//!
//! Everything here is GUI-free so the feature evaluator and its tests run headless.
//!
//! truck's booleans and the tessellation of their results are sensitive to tolerances
//! (measured on this commit, box 40 mm with a Ø10 through hole):
//! - the boolean wants a tolerance around 1 % of the part size; far below that it returns
//!   `None` or panics inside the kernel ("shell is not oriented and closed"),
//! - tessellating a boolean result panics (`IntersectionCurve::subs` unwraps a failed Newton
//!   search) when the mesh tolerance is smaller than the boolean tolerance,
//! - with the boolean at 1 % the mesh tolerance can go down to 0.01 mm and the volume of the
//!   holed box comes out within 0.01 % of the exact value.
//!
//! So [`boolean`] picks its tolerance from the operands' size and both it and [`tessellate`]
//! walk a ladder of coarser tolerances, catching panics, before giving up. The tolerance
//! that worked is reported so the log can say so.

use std::f64::consts::PI;
use std::panic::{catch_unwind, AssertUnwindSafe};
use truck_meshalgo::prelude::*;
use truck_modeling::*;

/// Mesh tolerance (max chord deviation, mm) for the viewport.
pub const MESH_TOL: f64 = 0.05;
/// Multipliers tried in turn when the kernel fails at the requested tolerance.
const LADDER: [f64; 5] = [1.0, 0.5, 2.0, 5.0, 10.0];
/// The same for meshing, one rung further (a coarse mesh beats no body).
const MESH_LADDER: [f64; 5] = [1.0, 2.0, 5.0, 10.0, 20.0];

#[derive(Debug, Clone, PartialEq)]
pub struct KernelError(pub String);

impl std::fmt::Display for KernelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type Result<T> = std::result::Result<T, KernelError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

impl BoolOp {
    pub fn label(self) -> &'static str {
        match self {
            BoolOp::Union => "UNION",
            BoolOp::Difference => "CUT",
            BoolOp::Intersection => "INTERSECT",
        }
    }
}

/// Run `f`, turning a kernel panic into an error (truck asserts and unwraps inside).
pub fn guarded<T>(what: &str, f: impl FnOnce() -> T) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        let msg = e
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        KernelError(format!("{what}: kernel panicked: {msg}"))
    })
}

/// Axis-aligned box with its minimum corner at `min`.
pub fn make_box(min: Point3, size: Vector3) -> Result<Solid> {
    if size.x <= 0.0 || size.y <= 0.0 || size.z <= 0.0 {
        return Err(KernelError(format!(
            "box size must be positive: {} x {} x {}",
            size.x, size.y, size.z
        )));
    }
    let v = builder::vertex(min);
    let e = builder::tsweep(&v, Vector3::unit_x() * size.x);
    let f = builder::tsweep(&e, Vector3::unit_y() * size.y);
    Ok(builder::tsweep(&f, Vector3::unit_z() * size.z))
}

/// Cylinder along `axis` (any length) starting at `base`.
pub fn make_cylinder(base: Point3, axis: Vector3, radius: f64, height: f64) -> Result<Solid> {
    if radius <= 0.0 || height <= 0.0 {
        return Err(KernelError(format!(
            "cylinder radius / height must be positive: r={radius} h={height}"
        )));
    }
    if axis.magnitude2() < 1e-12 {
        return Err(KernelError("cylinder axis is zero".into()));
    }
    let axis = axis.normalize();
    // any vector not parallel to the axis gives a radial direction
    let helper = if axis.x.abs() < 0.9 {
        Vector3::unit_x()
    } else {
        Vector3::unit_y()
    };
    let radial = axis.cross(helper).normalize();
    let v = builder::vertex(base + radial * radius);
    let circle: Wire = builder::rsweep(&v, base, axis, Rad(2.0 * PI), 4);
    let disk: Face = builder::try_attach_plane(&[circle])
        .map_err(|e| KernelError(format!("cylinder disk: {e}")))?;
    Ok(builder::tsweep(&disk, axis * height))
}

/// Diagonal of the bounding box of the solid's vertices (a scale for tolerances).
pub fn vertex_extent(solid: &Solid) -> f64 {
    let mut bb = BoundingBox::<Point3>::new();
    for shell in solid.boundaries() {
        for v in shell.vertex_iter() {
            bb.push(v.point());
        }
    }
    bb.diagonal().magnitude()
}

/// Boolean tolerance for two operands: 1 % of the smaller one, clamped to [0.01, 1] mm.
pub fn boolean_tol(a: &Solid, b: &Solid) -> f64 {
    (0.01 * vertex_extent(a).min(vertex_extent(b))).clamp(0.01, 1.0)
}

/// The result of a kernel step and the tolerance that finally worked.
#[derive(Debug, Clone)]
pub struct WithTol<T> {
    pub value: T,
    pub tol: f64,
    /// The tolerance had to be raised from the requested one.
    pub relaxed: bool,
    /// The tool body had to be scaled by this factor about its centre (coplanar faces).
    pub nudged: Option<f64>,
}

/// Scale factors tried on the tool body when faces coincide (truck cannot intersect
/// coplanar faces: "This wire is not simple"). 0.01 % moves a 100 mm face by 10 µm.
const NUDGES: [f64; 4] = [1.0 - 1e-4, 1.0 + 1e-4, 1.0 - 1e-3, 1.0 + 1e-3];

fn centre(solid: &Solid) -> Point3 {
    let mut bb = BoundingBox::<Point3>::new();
    for shell in solid.boundaries() {
        for v in shell.vertex_iter() {
            bb.push(v.point());
        }
    }
    bb.center()
}

/// A boolean result together with the mesh that proved it sound.
#[derive(Debug, Clone)]
pub struct Booled {
    pub solid: Solid,
    pub mesh: WithTol<MeshOut>,
    pub tol: f64,
    pub relaxed: bool,
    pub nudged: Option<f64>,
}

/// `a op b`, walking the tolerance ladder from [`boolean_tol`]; at each tolerance the exact
/// tool is tried first, then the tool nudged by [`NUDGES`] (coplanar faces). A candidate only
/// counts once it also tessellates at `mesh_tol` (walking [`MESH_LADDER`]): whether the
/// kernel can mesh a boolean result depends on the very same choices, so the two are searched
/// together and the mesh is kept.
pub fn boolean(op: BoolOp, a: &Solid, b: &Solid, mesh_tol: f64) -> Result<Booled> {
    let base = boolean_tol(a, b);
    let c = centre(b);
    // candidates in preference order: exact tool first, then nudged; base tolerance first
    let candidates: Vec<(usize, f64, Option<f64>, Solid)> = LADDER
        .iter()
        .enumerate()
        .flat_map(|(i, k)| {
            let tol = base * k;
            std::iter::once((i, tol, None, b.clone())).chain(NUDGES.iter().map(move |n| {
                (
                    i,
                    tol,
                    Some(*n),
                    builder::scaled(b, c, Vector3::new(*n, *n, *n)),
                )
            }))
        })
        .collect();
    let run = |tool: &Solid, tol: f64| -> Result<Option<Solid>> {
        guarded(op.label(), || match op {
            BoolOp::Union => truck_shapeops::or(a, tool, tol),
            BoolOp::Intersection => truck_shapeops::and(a, tool, tol),
            BoolOp::Difference => {
                let mut inv = tool.clone();
                inv.not();
                truck_shapeops::and(a, &inv, tol)
            }
        })
    };
    let mut last = KernelError("boolean: no attempt".into());
    let mut partial: Option<Booled> = None;
    // boolean results, computed once per candidate: `None` = failed
    let mut cache: Vec<Option<Option<Solid>>> = vec![None; candidates.len()];
    // outer loop: mesh fineness — a fine mesh with any boolean tolerance beats a coarse one
    for (ri, mk) in MESH_LADDER.iter().enumerate() {
        let mt = mesh_tol * mk;
        for (ci, (li, tol, nudge, tool)) in candidates.iter().enumerate() {
            if cache[ci].is_none() {
                cache[ci] = Some(match run(tool, *tol) {
                    Ok(Some(s)) => Some(s),
                    Ok(None) => {
                        last = KernelError(format!(
                            "{}: no result at tol {tol:.3} (surfaces may coincide)",
                            op.label()
                        ));
                        None
                    }
                    Err(e) => {
                        last = e;
                        None
                    }
                });
            }
            let Some(Some(solid)) = &cache[ci] else {
                continue;
            };
            match guarded("mesh", || finish(&solid.triangulation(mt))) {
                Ok(value) => {
                    let missing = value.missing;
                    let out = Booled {
                        solid: solid.clone(),
                        mesh: WithTol {
                            value,
                            tol: mt,
                            relaxed: ri > 0,
                            nudged: None,
                        },
                        tol: *tol,
                        relaxed: *li > 0,
                        nudged: *nudge,
                    };
                    if missing == 0 {
                        return Ok(out);
                    }
                    last = KernelError(format!(
                        "{}: {missing} faces of the result could not be meshed",
                        op.label()
                    ));
                    partial.get_or_insert(out);
                }
                Err(e) => last = e,
            }
        }
    }
    partial.ok_or(last)
}

/// A tessellated solid: the merged triangle mesh, which triangles belong to which B-rep face
/// (for picking / highlighting) and the B-rep edges as polylines (for the wireframe).
#[derive(Debug, Clone)]
pub struct MeshOut {
    pub mesh: PolygonMesh,
    /// Triangle index ranges per face, in `mesh.faces().tri_faces()` order.
    pub face_tris: Vec<std::ops::Range<usize>>,
    pub edges: Vec<Vec<Point3>>,
    /// Faces the kernel could not mesh (they are holes in `mesh`; volumes are then wrong).
    pub missing: usize,
}

impl MeshOut {
    /// Flat (per-corner) vertex arrays for the GPU: positions, normals, indices.
    pub fn flat(&self) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        let pos = self.mesh.positions();
        let nor = self.mesh.normals();
        let tris = self.mesh.faces().tri_faces();
        let mut positions = Vec::with_capacity(tris.len() * 3);
        let mut normals = Vec::with_capacity(tris.len() * 3);
        for t in tris {
            // per-face normal from the corners if the vertex has none
            let fallback = {
                let a = pos[t[0].pos];
                let b = pos[t[1].pos];
                let c = pos[t[2].pos];
                let n = (b - a).cross(c - a);
                if n.magnitude2() > 0.0 {
                    n.normalize()
                } else {
                    Vector3::unit_z()
                }
            };
            for v in t {
                let p = pos[v.pos];
                positions.push([p.x as f32, p.y as f32, p.z as f32]);
                let n = v.nor.map(|i| nor[i]).unwrap_or(fallback);
                normals.push([n.x as f32, n.y as f32, n.z as f32]);
            }
        }
        let indices = (0..positions.len() as u32).collect();
        (positions, normals, indices)
    }
    pub fn tri_count(&self) -> usize {
        self.mesh.faces().tri_faces().len()
    }
    pub fn edges_f32(&self) -> Vec<Vec<[f32; 3]>> {
        self.edges
            .iter()
            .map(|e| {
                e.iter()
                    .map(|p| [p.x as f32, p.y as f32, p.z as f32])
                    .collect()
            })
            .collect()
    }
}

/// Sort key for geometry: the kernel's face / triangle order varies between runs (parallel
/// iterators), and translucent fill blends in draw order, so the picture would flicker between
/// evaluations without a fixed order.
fn key(p: Point3) -> [i64; 3] {
    [p.x, p.y, p.z].map(|v| (v * 1e4).round() as i64)
}

fn finish(meshed: &<Solid as MeshableShape>::MeshedShape) -> MeshOut {
    let mut mesh = PolygonMesh::default();
    let mut face_tris = Vec::new();
    let mut missing = 0;
    let mut polys: Vec<([i64; 3], PolygonMesh)> = meshed
        .face_iter()
        .map(|face| {
            // faces the kernel could not mesh are `None`: skipped, but they still get a range
            let mut poly = face.surface().unwrap_or_else(|| {
                missing += 1;
                PolygonMesh::default()
            });
            if !face.orientation() {
                poly.invert();
            }
            poly.triangulate().remove_degenerate_faces();
            let centroid = {
                let pos = poly.positions();
                let tris = poly.faces().tri_faces();
                let n = (tris.len() * 3).max(1) as f64;
                let sum = tris
                    .iter()
                    .flat_map(|t| t.iter().map(|v| pos[v.pos].to_vec()))
                    .fold(Vector3::zero(), |a, b| a + b);
                Point3::from_vec(sum / n)
            };
            {
                let pos: Vec<Point3> = poly.positions().to_vec();
                poly.editor().faces.tri_faces_mut().sort_by_key(|t| {
                    key(Point3::from_vec(
                        (pos[t[0].pos].to_vec() + pos[t[1].pos].to_vec() + pos[t[2].pos].to_vec())
                            / 3.0,
                    ))
                });
            }
            (key(centroid), poly)
        })
        .collect();
    polys.sort_by_key(|(k, _)| *k);
    for (_, poly) in polys {
        let start = mesh.faces().tri_faces().len();
        let n = poly.faces().tri_faces().len();
        mesh.merge(poly);
        face_tris.push(start..start + n);
    }
    mesh.put_together_same_attrs(TOLERANCE)
        .remove_unused_attrs()
        .add_naive_normals(true);
    // shared edges come once per face: keep each id once
    let mut seen = std::collections::HashSet::new();
    let edges = meshed
        .edge_iter()
        .filter(|e| seen.insert(e.id()))
        .map(|e| e.curve().0.clone())
        .collect();
    MeshOut {
        mesh,
        face_tris,
        edges,
        missing,
    }
}

/// Triangulated, closed mesh with per-face normals plus edges, ready for rendering and STL.
/// Walks the tolerance ladder up from `tol` if the kernel fails.
pub fn tessellate(solid: &Solid, tol: f64) -> Result<WithTol<MeshOut>> {
    let mut last = KernelError("mesh: no attempt".into());
    let mut partial: Option<WithTol<MeshOut>> = None;
    for (i, k) in MESH_LADDER.iter().enumerate() {
        let t = tol * k;
        match guarded("mesh", || finish(&solid.triangulation(t))) {
            Ok(value) => {
                let out = WithTol {
                    value,
                    tol: t,
                    relaxed: i > 0,
                    nudged: None,
                };
                if out.value.missing == 0 {
                    return Ok(out);
                }
                last = KernelError(format!(
                    "mesh: {} faces could not be meshed at tol {t:.3}",
                    out.value.missing
                ));
                partial.get_or_insert(out);
            }
            Err(e) => last = e,
        }
    }
    // a body with holes beats no body: the caller reports `missing`
    partial.ok_or(last)
}

/// Move a solid.
pub fn translate(solid: &Solid, by: Vector3) -> Solid {
    builder::translated(solid, by)
}

/// Rotate a solid about the axis through `origin` (degrees).
pub fn rotate(solid: &Solid, origin: Point3, axis: Vector3, degrees: f64) -> Result<Solid> {
    if axis.magnitude2() < 1e-12 {
        return Err(KernelError("rotation axis is zero".into()));
    }
    Ok(builder::rotated(
        solid,
        origin,
        axis.normalize(),
        Rad(degrees.to_radians()),
    ))
}

/// Signed volume in mm³ and centre of gravity of a closed mesh.
pub fn volume_and_centroid(mesh: &PolygonMesh) -> (f64, Point3) {
    let v = mesh.volume();
    let c = mesh.center_of_gravity();
    (v, Point3::from_homogeneous(c))
}

/// Binary STL.
pub fn write_stl<W: std::io::Write>(mesh: &PolygonMesh, w: &mut W) -> Result<()> {
    truck_polymesh::stl::write(mesh, w, truck_polymesh::stl::StlType::Binary)
        .map_err(|e| KernelError(format!("stl: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1.0)
    }

    fn mesh(s: &Solid) -> PolygonMesh {
        let m = tessellate(s, MESH_TOL).unwrap();
        assert!(!m.relaxed, "mesh tolerance had to be relaxed to {}", m.tol);
        m.value.mesh
    }

    #[test]
    fn box_volume() {
        let b = make_box(Point3::origin(), Vector3::new(20.0, 30.0, 40.0)).unwrap();
        let mesh = mesh(&b);
        let (v, c) = volume_and_centroid(&mesh);
        assert!(near(v, 24_000.0, 1e-6), "v={v}");
        assert!(near(c.x, 10.0, 1e-6) && near(c.y, 15.0, 1e-6) && near(c.z, 20.0, 1e-6));
        assert_eq!(mesh.faces().quad_faces().len(), 0);
        assert!(near(
            vertex_extent(&b),
            (400.0f64 + 900.0 + 1600.0).sqrt(),
            1e-9
        ));
        let out = tessellate(&b, MESH_TOL).unwrap().value;
        assert_eq!(out.face_tris.len(), 6);
        assert_eq!(out.edges.len(), 12);
        assert!(out.face_tris.iter().all(|r| r.len() == 2));
        let (p, n, i) = out.flat();
        assert_eq!((p.len(), n.len(), i.len()), (36, 36, 36));
    }

    #[test]
    fn cylinder_volume() {
        let cyl = make_cylinder(Point3::origin(), Vector3::unit_z(), 10.0, 50.0).unwrap();
        let (v, _) = volume_and_centroid(&mesh(&cyl));
        // polygonal approximation of the circle is a little under πr²h
        assert!(near(v, PI * 100.0 * 50.0, 5e-3), "v={v}");
    }

    #[test]
    fn box_minus_cylinder_through_hole() {
        let b = make_box(Point3::origin(), Vector3::new(40.0, 40.0, 10.0)).unwrap();
        let hole =
            make_cylinder(Point3::new(20.0, 20.0, -5.0), Vector3::unit_z(), 5.0, 20.0).unwrap();
        let cut = boolean(BoolOp::Difference, &b, &hole, MESH_TOL).unwrap();
        assert!(!cut.relaxed);
        let mesh = cut.mesh.value.mesh;
        let (v, _) = volume_and_centroid(&mesh);
        assert!(near(v, 16_000.0 - PI * 25.0 * 10.0, 5e-3), "v={v}");
        let mut buf = Vec::new();
        write_stl(&mesh, &mut buf).unwrap();
        assert_eq!(buf.len(), 84 + 50 * mesh.faces().tri_faces().len());
    }

    #[test]
    fn small_part_hole() {
        // 4 mm part with a Ø1 hole: the boolean tolerance scales down with the part
        let b = make_box(Point3::origin(), Vector3::new(4.0, 4.0, 1.0)).unwrap();
        let hole = make_cylinder(Point3::new(2.0, 2.0, -0.5), Vector3::unit_z(), 0.5, 2.0).unwrap();
        let cut = boolean(BoolOp::Difference, &b, &hole, 0.01).unwrap();
        let (v, _) = volume_and_centroid(&cut.mesh.value.mesh);
        assert!(near(v, 16.0 - PI * 0.25, 5e-3), "v={v}");
    }

    #[test]
    fn union_and_intersection_of_overlapping_boxes() {
        let a = make_box(Point3::origin(), Vector3::new(20.0, 20.0, 20.0)).unwrap();
        let b = make_box(Point3::new(10.0, 5.0, 5.0), Vector3::new(20.0, 10.0, 10.0)).unwrap();
        let u = boolean(BoolOp::Union, &a, &b, MESH_TOL).unwrap();
        let (v, _) = volume_and_centroid(&u.mesh.value.mesh);
        assert!(near(v, 8000.0 + 2000.0 - 1000.0, 1e-3), "v={v}");
        let i = boolean(BoolOp::Intersection, &a, &b, MESH_TOL).unwrap();
        let (v, _) = volume_and_centroid(&i.mesh.value.mesh);
        assert!(near(v, 1000.0, 1e-3), "v={v}");
    }

    #[test]
    fn transforms() {
        let b = make_box(Point3::origin(), Vector3::new(10.0, 20.0, 30.0)).unwrap();
        let moved = translate(&b, Vector3::new(5.0, 0.0, 0.0));
        let (_, c) = volume_and_centroid(&mesh(&moved));
        assert!(near(c.x, 10.0, 1e-6));
        let turned = rotate(&b, Point3::origin(), Vector3::unit_z(), 90.0).unwrap();
        let (v, c) = volume_and_centroid(&mesh(&turned));
        assert!(near(v, 6000.0, 1e-6));
        assert!(near(c.x, -10.0, 1e-6) && near(c.y, 5.0, 1e-6), "{c:?}");
    }

    #[test]
    fn coplanar_boxes_need_a_nudge() {
        // plate + wall sharing the base plane and a side face
        let a = make_box(Point3::origin(), Vector3::new(60.0, 40.0, 6.0)).unwrap();
        let b = make_box(Point3::origin(), Vector3::new(6.0, 40.0, 30.0)).unwrap();
        let u = boolean(BoolOp::Union, &a, &b, MESH_TOL).unwrap();
        assert!(u.nudged.is_some() && !u.relaxed);
        let (v, _) = volume_and_centroid(&u.mesh.value.mesh);
        assert!(near(v, 14_400.0 + 6.0 * 40.0 * 24.0, 1e-3), "v={v}");
        let d = boolean(BoolOp::Difference, &a, &b, MESH_TOL).unwrap();
        let (v, _) = volume_and_centroid(&d.mesh.value.mesh);
        assert!(near(v, 14_400.0 - 6.0 * 40.0 * 6.0, 1e-3), "v={v}");
    }

    /// The M8-ish hex bolt: three rotated blanks intersected, a shank unioned. The union used
    /// to succeed but its result could not be meshed at any rung; searching the boolean and
    /// mesh tolerances together finds a combination that works.
    #[test]
    fn hex_bolt_meshes() {
        let (af, head, d, len) = (13.0, 4.0, 6.0, 30.0);
        let blank = || {
            make_box(
                Point3::new(-af / 2.0, -30.0, 0.0),
                Vector3::new(af, 60.0, head),
            )
            .unwrap()
        };
        let b = rotate(&blank(), Point3::origin(), Vector3::unit_z(), 60.0).unwrap();
        let c = rotate(&blank(), Point3::origin(), Vector3::unit_z(), 120.0).unwrap();
        let ab = boolean(BoolOp::Intersection, &blank(), &b, MESH_TOL).unwrap();
        let hex = boolean(BoolOp::Intersection, &ab.solid, &c, MESH_TOL).unwrap();
        let (v, _) = volume_and_centroid(&hex.mesh.value.mesh);
        assert!(
            near(v, 3f64.sqrt() / 2.0 * af * af * head, 1e-3),
            "hex v={v}"
        );
        let shank = make_cylinder(
            Point3::new(0.0, 0.0, head - 1.0),
            Vector3::unit_z(),
            d / 2.0,
            len + 1.0,
        )
        .unwrap();
        let bolt = boolean(BoolOp::Union, &hex.solid, &shank, MESH_TOL).unwrap();
        eprintln!(
            "bolt: tol {} nudged {:?} mesh tol {} missing {}",
            bolt.tol, bolt.nudged, bolt.mesh.tol, bolt.mesh.value.missing
        );
        assert_eq!(bolt.mesh.value.missing, 0);
        let (v, _) = volume_and_centroid(&bolt.mesh.value.mesh);
        let want = 3f64.sqrt() / 2.0 * af * af * head + PI * 9.0 * len;
        assert!(near(v, want, 1e-2), "bolt v={v} want {want}");
    }

    #[test]
    fn bad_sizes_are_errors() {
        assert!(make_box(Point3::origin(), Vector3::new(0.0, 1.0, 1.0)).is_err());
        assert!(make_cylinder(Point3::origin(), Vector3::unit_z(), 1.0, 0.0).is_err());
        assert!(make_cylinder(Point3::origin(), Vector3::zero(), 1.0, 1.0).is_err());
    }

    #[test]
    fn guarded_turns_panics_into_errors() {
        let e = guarded("probe", || -> u8 { panic!("boom") }).unwrap_err();
        assert_eq!(e.0, "probe: kernel panicked: boom");
    }
}
