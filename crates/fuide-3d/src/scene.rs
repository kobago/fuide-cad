//! CPU-side scene: triangle meshes and line batches. Bump `version` after changing anything so
//! the renderer re-uploads.

use crate::math::Vec3;

/// Flat-shaded triangle mesh: one normal per vertex (duplicate vertices per face for hard edges).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    /// Tint mixed into the hologram colour (rgb) and opacity multiplier (a). White = plain.
    pub color: [f32; 4],
}

impl MeshData {
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        bounds(self.positions.iter().copied())
    }
}

/// Segments (`points` is pairs) drawn as screen-space quads `width` px wide.
#[derive(Clone, Debug, PartialEq)]
pub struct LineBatch {
    pub points: Vec<[f32; 3]>,
    pub width: f32,
    /// Straight rgba (the shader premultiplies).
    pub color: [f32; 4],
    /// Hidden behind meshes (`true`) or always visible.
    pub depth_test: bool,
}

impl LineBatch {
    pub fn new(width: f32, color: [f32; 4]) -> Self {
        Self {
            points: Vec::new(),
            width,
            color,
            depth_test: true,
        }
    }
    pub fn segment(&mut self, a: [f32; 3], b: [f32; 3]) -> &mut Self {
        self.points.push(a);
        self.points.push(b);
        self
    }
    pub fn polyline(&mut self, pts: &[[f32; 3]]) -> &mut Self {
        for w in pts.windows(2) {
            self.segment(w[0], w[1]);
        }
        self
    }
    /// XY grid at z = 0: `extent` mm each side, `step` mm apart.
    pub fn grid(extent: f32, step: f32, color: [f32; 4]) -> Self {
        let mut b = LineBatch::new(1.0, color);
        let n = (extent / step).round() as i32;
        for i in -n..=n {
            let t = i as f32 * step;
            b.segment([t, -extent, 0.0], [t, extent, 0.0]);
            b.segment([-extent, t, 0.0], [extent, t, 0.0]);
        }
        b
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub meshes: Vec<MeshData>,
    /// Edges of the meshes: hidden behind the fill, drawn again faintly in X-ray.
    pub edges: Vec<LineBatch>,
    /// Grid, axes, guides: drawn once, depth-tested.
    pub overlay: Vec<LineBatch>,
    pub version: u64,
}

impl Scene {
    pub fn clear(&mut self) {
        self.meshes.clear();
        self.edges.clear();
        self.overlay.clear();
        self.version += 1;
    }
    pub fn bump(&mut self) {
        self.version += 1;
    }
    /// Bounding box of the meshes (not the overlay).
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        bounds(self.meshes.iter().flat_map(|m| m.positions.iter().copied()))
    }
}

fn bounds(points: impl Iterator<Item = [f32; 3]>) -> Option<(Vec3, Vec3)> {
    let mut it = points.map(Vec3::from_array);
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

/// A unit-ish cube for tests and placeholders.
pub fn cube(min: Vec3, max: Vec3) -> MeshData {
    let mut m = MeshData {
        color: [1.0; 4],
        ..Default::default()
    };
    let corners = |axis: usize, sign: f32| -> [Vec3; 4] {
        let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut out = [Vec3::ZERO; 4];
        let quad = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        for (i, (qa, qb)) in quad.iter().enumerate() {
            let mut v = [0.0f32; 3];
            v[axis] = sign;
            v[a] = *qa * sign; // keep winding outward
            v[b] = *qb;
            let vv = Vec3::from_array(v);
            out[i] = Vec3::new(
                if vv.x > 0.0 { max.x } else { min.x },
                if vv.y > 0.0 { max.y } else { min.y },
                if vv.z > 0.0 { max.z } else { min.z },
            );
        }
        out
    };
    for axis in 0..3 {
        for sign in [1.0f32, -1.0] {
            let q = corners(axis, sign);
            let mut n = [0.0; 3];
            n[axis] = sign;
            let base = m.positions.len() as u32;
            for p in q {
                m.positions.push(p.to_array());
                m.normals.push(n);
            }
            m.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_has_12_triangles_and_bounds() {
        let c = cube(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(c.indices.len(), 36);
        assert_eq!(c.positions.len(), 24);
        let (lo, hi) = c.bounds().unwrap();
        assert_eq!(lo, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(hi, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn grid_line_count() {
        let g = LineBatch::grid(10.0, 5.0, [1.0; 4]);
        assert_eq!(g.points.len(), 2 * 2 * 5);
    }
}
