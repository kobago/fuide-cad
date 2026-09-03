//! Evaluate a [`Document`] into bodies: run the features in order through the mesh kernel
//! ([`crate::mesh`], Manifold), keep going past failures, and mesh the result bodies. GUI-free;
//! the app runs this on a worker thread.
//!
//! The B-rep kernel (`crate::kernel`, truck) is kept for a later STEP export; modelling and
//! display go through Manifold, whose booleans are exact and do not mind coplanar faces.

use crate::doc::{Axis, BoolOp, Document, Feature, FeatureId, FeatureKind};
use crate::expr;
use crate::mesh::{self, MeshOut};
use manifold_rust::manifold::Manifold;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Dihedral angle (degrees) above which an edge is drawn in the wireframe.
pub const CREASE_DEG: f64 = 30.0;

#[derive(Debug, Clone)]
pub struct Body {
    pub feature: FeatureId,
    pub name: String,
    pub mesh: MeshOut,
    pub volume: f64,
    pub centroid: [f64; 3],
    pub bbox: ([f64; 3], [f64; 3]),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureError {
    pub feature: FeatureId,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Evaluation {
    /// Result bodies (features not consumed by a later one), in feature order.
    pub bodies: Vec<Body>,
    pub errors: Vec<FeatureError>,
    /// Non-fatal remarks.
    pub notes: Vec<String>,
    pub elapsed: Duration,
    /// `Document` revision this was computed from, so the UI can tell stale results apart.
    pub revision: u64,
}

impl Evaluation {
    pub fn error_for(&self, id: FeatureId) -> Option<&str> {
        self.errors
            .iter()
            .find(|e| e.feature == id)
            .map(|e| e.message.as_str())
    }
    pub fn body_for(&self, id: FeatureId) -> Option<&Body> {
        self.bodies.iter().find(|b| b.feature == id)
    }
    pub fn tri_count(&self) -> usize {
        self.bodies.iter().map(|b| b.mesh.tri_count).sum()
    }
    pub fn bbox(&self) -> Option<([f64; 3], [f64; 3])> {
        let mut it = self.bodies.iter().map(|b| b.bbox);
        let first = it.next()?;
        Some(it.fold(first, |(lo, hi), (a, b)| {
            (
                [lo[0].min(a[0]), lo[1].min(a[1]), lo[2].min(a[2])],
                [hi[0].max(b[0]), hi[1].max(b[1]), hi[2].max(b[2])],
            )
        }))
    }
}

/// Resolve the document's parameters (each may use the ones before it).
pub fn resolve_params(doc: &Document) -> (HashMap<String, f64>, Vec<String>) {
    let mut values: HashMap<String, f64> = HashMap::new();
    let mut errors = Vec::new();
    for p in &doc.params {
        let lookup = |n: &str| values.get(n).copied();
        match expr::eval(&p.value, &lookup) {
            Ok(v) => {
                values.insert(p.name.clone(), v);
            }
            Err(e) => errors.push(format!("parameter {} = {}: {e}", p.name, p.value)),
        }
    }
    (values, errors)
}

fn num(src: &str, vars: &HashMap<String, f64>, what: &str) -> Result<f64, String> {
    expr::eval(src, &|n| vars.get(n).copied()).map_err(|e| format!("{what} = {src}: {e}"))
}

fn vec3(v: &[String; 3], vars: &HashMap<String, f64>, what: &str) -> Result<[f64; 3], String> {
    Ok([
        num(&v[0], vars, &format!("{what}.x"))?,
        num(&v[1], vars, &format!("{what}.y"))?,
        num(&v[2], vars, &format!("{what}.z"))?,
    ])
}

fn build(
    f: &Feature,
    vars: &HashMap<String, f64>,
    solids: &HashMap<FeatureId, Manifold>,
    tol: f64,
) -> Result<Manifold, String> {
    let input = |id: FeatureId| -> Result<&Manifold, String> {
        solids
            .get(&id)
            .ok_or_else(|| format!("input #{id} has no body (failed, suppressed or missing)"))
    };
    match &f.kind {
        FeatureKind::Box { origin, size } => {
            let o = vec3(origin, vars, "origin")?;
            let s = vec3(size, vars, "size")?;
            mesh::make_box(o, s).map_err(|e| e.0)
        }
        FeatureKind::Cylinder {
            base,
            axis,
            radius,
            height,
        } => {
            let b = vec3(base, vars, "base")?;
            let r = num(radius, vars, "radius")?;
            let h = num(height, vars, "height")?;
            mesh::make_cylinder(b, axis.unit(), r, h, tol).map_err(|e| e.0)
        }
        FeatureKind::Thread {
            base,
            axis,
            diameter,
            pitch,
            length,
        } => {
            let b = vec3(base, vars, "base")?;
            let d = num(diameter, vars, "diameter")?;
            let p = num(pitch, vars, "pitch")?;
            let l = num(length, vars, "length")?;
            let th = mesh::thread(d, p, l, tol).map_err(|e| e.0)?;
            let th = if *axis == Axis::Z {
                th
            } else {
                // built along +Z: turn it onto the axis first
                let (rot_axis, deg) = match axis {
                    Axis::X => ([0.0, 1.0, 0.0], 90.0),
                    Axis::Y => ([1.0, 0.0, 0.0], -90.0),
                    Axis::Z => unreachable!(),
                };
                mesh::rotate(&th, [0.0; 3], rot_axis, deg).map_err(|e| e.0)?
            };
            Ok(mesh::translate(&th, b))
        }
        FeatureKind::Gear {
            center,
            axis,
            module,
            teeth,
            width,
            pressure,
        } => {
            let c = vec3(center, vars, "center")?;
            let m = num(module, vars, "module")?;
            let z = num(teeth, vars, "teeth")?;
            let w = num(width, vars, "width")?;
            let a = num(pressure, vars, "pressure")?;
            if z < 4.0 || z.fract().abs() > 1e-9 {
                return Err(format!(
                    "teeth = {teeth}: must be a whole number of at least 4"
                ));
            }
            let g = mesh::gear(m, z as u32, a, w, tol).map_err(|e| e.0)?;
            let g = match axis {
                Axis::Z => g,
                Axis::X => mesh::rotate(&g, [0.0; 3], [0.0, 1.0, 0.0], 90.0).map_err(|e| e.0)?,
                Axis::Y => mesh::rotate(&g, [0.0; 3], [1.0, 0.0, 0.0], -90.0).map_err(|e| e.0)?,
            };
            Ok(mesh::translate(&g, c))
        }
        FeatureKind::Boolean { op, a, b } => {
            let (sa, sb) = (input(*a)?, input(*b)?);
            let kop = match op {
                BoolOp::Union => mesh::BoolOp::Union,
                BoolOp::Cut => mesh::BoolOp::Difference,
                BoolOp::Intersect => mesh::BoolOp::Intersection,
            };
            mesh::boolean(kop, sa, sb).map_err(|e| e.0)
        }
        FeatureKind::Translate { target, by } => {
            let s = input(*target)?;
            Ok(mesh::translate(s, vec3(by, vars, "by")?))
        }
        FeatureKind::Rotate {
            target,
            origin,
            axis,
            angle,
        } => {
            let s = input(*target)?;
            let o = vec3(origin, vars, "origin")?;
            let deg = num(angle, vars, "angle")?;
            mesh::rotate(s, o, axis.unit(), deg).map_err(|e| e.0)
        }
    }
}

/// Run every feature; result bodies are meshed at chord tolerance `tol`.
pub fn evaluate(doc: &Document, tol: f64, revision: u64) -> Evaluation {
    let start = Instant::now();
    let mut out = Evaluation {
        revision,
        ..Default::default()
    };
    let (vars, perrs) = resolve_params(doc);
    out.errors
        .extend(perrs.into_iter().map(|message| FeatureError {
            feature: 0,
            message,
        }));
    let mut solids: HashMap<FeatureId, Manifold> = HashMap::new();
    for f in &doc.features {
        if f.suppressed {
            continue;
        }
        let built = crate::kernel::guarded(&f.name, || build(f, &vars, &solids, tol))
            .map_err(|e| e.0)
            .and_then(|r| r);
        match built {
            Ok(s) => {
                if s.is_empty() {
                    out.notes
                        .push(format!("{} (#{}): the result is empty", f.name, f.id));
                }
                solids.insert(f.id, s);
            }
            Err(message) => out.errors.push(FeatureError {
                feature: f.id,
                message,
            }),
        }
    }
    for f in &doc.features {
        if !doc.is_result(f.id) {
            continue;
        }
        let Some(solid) = solids.get(&f.id) else {
            continue;
        };
        if solid.is_empty() {
            continue;
        }
        let meshed = crate::kernel::guarded("mesh", || {
            let m = mesh::mesh_out(solid, CREASE_DEG);
            let (volume, centroid) = mesh::volume_and_centroid(solid);
            (m, volume, centroid, mesh::bbox(solid))
        });
        match meshed {
            Ok((m, volume, centroid, bbox)) => out.bodies.push(Body {
                feature: f.id,
                name: f.name.clone(),
                mesh: m,
                volume,
                centroid,
                bbox,
            }),
            Err(e) => out.errors.push(FeatureError {
                feature: f.id,
                message: e.0,
            }),
        }
    }
    out.elapsed = start.elapsed();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::xyz;
    use std::f64::consts::PI;

    fn near(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1.0)
    }

    fn plate() -> Document {
        let mut d = Document::new("plate");
        d.set_param("w", "40").unwrap();
        d.set_param("hole", "w / 8").unwrap();
        let plate = d.add(
            FeatureKind::Box {
                origin: xyz(0.0, 0.0, 0.0),
                size: ["w".into(), "w".into(), "10".into()],
            },
            Some("PLATE"),
        );
        let hole = d.add(
            FeatureKind::Cylinder {
                base: ["w/2".into(), "w/2".into(), "-5".into()],
                axis: Axis::Z,
                radius: "hole".into(),
                height: "20".into(),
            },
            Some("HOLE"),
        );
        d.add(
            FeatureKind::Boolean {
                op: BoolOp::Cut,
                a: plate,
                b: hole,
            },
            None,
        );
        d
    }

    #[test]
    fn plate_with_hole() {
        let ev = evaluate(&plate(), mesh::MESH_TOL, 7);
        assert!(ev.errors.is_empty(), "{:?}", ev.errors);
        assert_eq!(ev.bodies.len(), 1);
        assert_eq!(ev.revision, 7);
        let b = &ev.bodies[0];
        assert_eq!(b.feature, 3);
        assert!(
            near(b.volume, 16_000.0 - PI * 25.0 * 10.0, 3e-3),
            "{}",
            b.volume
        );
        for (got, want) in b
            .bbox
            .0
            .iter()
            .chain(b.bbox.1.iter())
            .zip([0.0, 0.0, 0.0, 40.0, 40.0, 10.0])
        {
            assert!(near(*got, want, 1e-9), "{:?}", b.bbox);
        }
        assert!(b.mesh.tri_count > 12 && !b.mesh.edges.is_empty());
        assert!(near(ev.bbox().unwrap().1[2], 10.0, 1e-9));
    }

    #[test]
    fn suppressed_cut_shows_both_inputs() {
        let mut d = plate();
        d.get_mut(3).unwrap().suppressed = true;
        let ev = evaluate(&d, mesh::MESH_TOL, 1);
        assert!(ev.errors.is_empty());
        assert_eq!(
            ev.bodies.iter().map(|b| b.feature).collect::<Vec<_>>(),
            [1, 2]
        );
    }

    #[test]
    fn errors_name_the_feature_and_keep_going() {
        let mut d = plate();
        d.get_mut(2).unwrap().kind.set_field("radius", "r").unwrap();
        let ev = evaluate(&d, mesh::MESH_TOL, 1);
        assert_eq!(
            ev.error_for(2).unwrap(),
            "radius = r: unknown parameter 'r'"
        );
        assert!(ev.error_for(3).unwrap().contains("input #2 has no body"));
        assert!(ev.bodies.is_empty());
        d.set_param("r", "3").unwrap();
        let ev = evaluate(&d, mesh::MESH_TOL, 2);
        assert!(ev.errors.is_empty(), "{:?}", ev.errors);
    }

    #[test]
    fn bad_parameter_reports_without_feature() {
        let mut d = plate();
        d.set_param("w", "1 /").unwrap();
        let ev = evaluate(&d, mesh::MESH_TOL, 1);
        assert_eq!(ev.errors[0].feature, 0);
        assert!(ev.errors[0].message.starts_with("parameter w = 1 /:"));
    }

    #[test]
    fn transforms_move_the_body() {
        let mut d = Document::new("t");
        let b = d.add(
            FeatureKind::Box {
                origin: xyz(0.0, 0.0, 0.0),
                size: xyz(10.0, 10.0, 10.0),
            },
            None,
        );
        let t = d.add(
            FeatureKind::Translate {
                target: b,
                by: xyz(100.0, 0.0, 0.0),
            },
            None,
        );
        d.add(
            FeatureKind::Rotate {
                target: t,
                origin: xyz(0.0, 0.0, 0.0),
                axis: Axis::Z,
                angle: "90".into(),
            },
            None,
        );
        let ev = evaluate(&d, mesh::MESH_TOL, 1);
        assert!(ev.errors.is_empty(), "{:?}", ev.errors);
        let c = ev.bodies[0].centroid;
        assert!(near(c[0], -5.0, 1e-6) && near(c[1], 105.0, 1e-6), "{c:?}");
    }

    #[test]
    fn gear_feature_with_a_bore() {
        let mut d = Document::new("g");
        d.set_param("m", "1.5").unwrap();
        let g = d.add(
            FeatureKind::Gear {
                center: xyz(0.0, 0.0, 0.0),
                axis: Axis::Z,
                module: "m".into(),
                teeth: "12".into(),
                width: "8".into(),
                pressure: "20".into(),
            },
            Some("PINION"),
        );
        let bore = d.add(
            FeatureKind::Cylinder {
                base: xyz(0.0, 0.0, -1.0),
                axis: Axis::Z,
                radius: "2.5".into(),
                height: "10".into(),
            },
            None,
        );
        d.add(
            FeatureKind::Boolean {
                op: BoolOp::Cut,
                a: g,
                b: bore,
            },
            None,
        );
        let ev = evaluate(&d, mesh::MESH_TOL, 1);
        assert!(ev.errors.is_empty(), "{:?}", ev.errors);
        let b = &ev.bodies[0];
        assert!(near(b.bbox.1[0], 1.5 * 6.0 + 1.5, 1e-9), "{:?}", b.bbox);
        assert!(near(b.bbox.1[2], 8.0, 1e-9));
        d.get_mut(g)
            .unwrap()
            .kind
            .set_field("teeth", "12.5")
            .unwrap();
        let ev = evaluate(&d, mesh::MESH_TOL, 2);
        assert!(ev.error_for(g).unwrap().contains("whole number"));
    }

    #[test]
    fn thread_on_a_shank_along_each_axis() {
        for axis in Axis::ALL {
            let mut d = Document::new("t");
            d.add(
                FeatureKind::Thread {
                    base: xyz(1.0, 2.0, 3.0),
                    axis,
                    diameter: "8".into(),
                    pitch: "1.25".into(),
                    length: "20".into(),
                },
                None,
            );
            let ev = evaluate(&d, mesh::MESH_TOL, 1);
            assert!(ev.errors.is_empty(), "{:?}", ev.errors);
            let b = &ev.bodies[0];
            let along = match axis {
                Axis::X => 0,
                Axis::Y => 1,
                Axis::Z => 2,
            };
            let base = [1.0, 2.0, 3.0];
            assert!(
                near(b.bbox.0[along], base[along], 1e-6)
                    && near(b.bbox.1[along], base[along] + 20.0, 1e-6),
                "{axis:?} {:?}",
                b.bbox
            );
            for (k, origin) in base.iter().enumerate().filter(|(k, _)| *k != along) {
                assert!(
                    near(b.bbox.0[k], origin - 4.0, 1e-3) && near(b.bbox.1[k], origin + 4.0, 1e-3),
                    "{axis:?} {:?}",
                    b.bbox
                );
            }
        }
    }
}
