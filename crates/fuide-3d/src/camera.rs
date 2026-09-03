//! Z-up orbit camera.

use crate::math::{self, Mat4, Vec3};
use egui::{pos2, Pos2, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Iso,
    Front,
    Top,
    Right,
}

impl Preset {
    pub fn label(self) -> &'static str {
        match self {
            Preset::Iso => "ISO",
            Preset::Front => "FRONT",
            Preset::Top => "TOP",
            Preset::Right => "RIGHT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    /// Around Z, radians. 0 = looking from +X.
    pub yaw: f32,
    /// Above the XY plane, radians.
    pub pitch: f32,
    pub dist: f32,
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let mut c = Self {
            target: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            dist: 300.0,
            fov_y: 40f32.to_radians(),
        };
        c.set_preset(Preset::Iso);
        c
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        self.target + Vec3::new(cp * cy, cp * sy, sp) * self.dist
    }
    pub fn forward(&self) -> Vec3 {
        (self.target - self.eye()).normalize()
    }
    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Z).normalize()
    }
    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward())
    }
    pub fn near_far(&self) -> (f32, f32) {
        (self.dist * 0.01, self.dist * 50.0)
    }
    pub fn view(&self) -> Mat4 {
        math::look_at(self.eye(), self.target, Vec3::Z)
    }
    pub fn proj(&self, aspect: f32) -> Mat4 {
        let (n, f) = self.near_far();
        math::perspective(self.fov_y, aspect.max(1e-3), n, f)
    }
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        math::mul(&self.proj(aspect), &self.view())
    }

    /// Drag in pixels → yaw / pitch.
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.55, 1.55);
    }
    /// Drag in pixels → move the target in the view plane.
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_h: f32) {
        let world_per_px = 2.0 * self.dist * (self.fov_y * 0.5).tan() / viewport_h.max(1.0);
        self.target =
            self.target - self.right() * (dx * world_per_px) + self.up() * (dy * world_per_px);
    }
    /// `factor` > 1 zooms in.
    pub fn zoom(&mut self, factor: f32) {
        self.dist = (self.dist / factor.max(1e-3)).clamp(0.5, 1.0e6);
    }
    pub fn set_preset(&mut self, p: Preset) {
        let (yaw, pitch) = match p {
            Preset::Iso => (-135f32.to_radians(), 30f32.to_radians()),
            Preset::Front => (-90f32.to_radians(), 0.0),
            Preset::Top => (-90f32.to_radians(), 89.9f32.to_radians()),
            Preset::Right => (0.0, 0.0),
        };
        self.yaw = yaw;
        self.pitch = pitch;
    }
    /// Look at the box `min..max` from the current direction so it fills the view.
    pub fn fit(&mut self, min: Vec3, max: Vec3) {
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(1.0);
        self.target = center;
        self.dist = radius / (self.fov_y * 0.5).sin() * 1.15;
    }

    /// World point → pixel in `rect`, `None` behind the camera.
    pub fn project(&self, p: Vec3, rect: Rect) -> Option<Pos2> {
        let clip = math::transform(&self.view_proj(rect.aspect_ratio()), p);
        if clip[3] <= 1e-6 {
            return None;
        }
        let nx = clip[0] / clip[3];
        let ny = clip[1] / clip[3];
        Some(pos2(
            rect.center().x + nx * rect.width() * 0.5,
            rect.center().y - ny * rect.height() * 0.5,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_eye_is_above_and_in_front() {
        let c = OrbitCamera::default();
        let e = c.eye();
        assert!(e.z > 0.0 && e.x < 0.0 && e.y < 0.0);
    }

    #[test]
    fn fit_centres_and_projects_inside() {
        let mut c = OrbitCamera::default();
        c.fit(Vec3::new(-10.0, -10.0, 0.0), Vec3::new(10.0, 10.0, 20.0));
        let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let centre = c.project(c.target, rect).unwrap();
        assert!((centre.x - 200.0).abs() < 1e-2 && (centre.y - 150.0).abs() < 1e-2);
        for corner in [
            Vec3::new(-10.0, -10.0, 0.0),
            Vec3::new(10.0, 10.0, 20.0),
            Vec3::new(10.0, -10.0, 20.0),
        ] {
            let p = c.project(corner, rect).unwrap();
            assert!(rect.contains(p), "{corner:?} → {p:?}");
        }
    }

    #[test]
    fn zoom_and_orbit_are_bounded() {
        let mut c = OrbitCamera::default();
        for _ in 0..100 {
            c.zoom(10.0);
            c.orbit(0.0, 1000.0);
        }
        assert!(c.dist >= 0.5 && c.pitch <= 1.55);
    }
}
