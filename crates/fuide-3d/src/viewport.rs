//! The egui widget: orbit / pan / zoom input, the render call, the axis triad HUD.

use crate::camera::OrbitCamera;
use crate::math::Vec3;
use crate::renderer::{Renderer, Style};
use crate::scene::Scene;
use egui::{pos2, vec2, Color32, PointerButton, Rect, Response, Sense, Stroke, Ui};
use fuide::theme::Palette;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Shaded,
    Wire,
    Xray,
}

impl ViewMode {
    pub const ALL: [ViewMode; 3] = [ViewMode::Shaded, ViewMode::Wire, ViewMode::Xray];
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Shaded => "SHADED",
            ViewMode::Wire => "WIRE",
            ViewMode::Xray => "X-RAY",
        }
    }
    /// The renderer style for this mode in `pal`.
    pub fn style(self, pal: &Palette) -> Style {
        let c = pal.accent;
        let accent = [
            c.r() as f32 / 255.0,
            c.g() as f32 / 255.0,
            c.b() as f32 / 255.0,
        ];
        let base = Style {
            accent,
            ..Style::default()
        };
        match self {
            ViewMode::Shaded => base,
            ViewMode::Wire => Style {
                fill_alpha: 0.0,
                hidden_alpha: 0.18,
                ..base
            },
            ViewMode::Xray => Style {
                fill_alpha: 0.28,
                hidden_alpha: 0.3,
                ..base
            },
        }
    }
}

/// Owns the camera and the GPU renderer (attached once the app has a `RenderState`).
pub struct Viewport {
    pub camera: OrbitCamera,
    pub mode: ViewMode,
    renderer: Option<Renderer>,
    render_state: Option<egui_wgpu::RenderState>,
    /// Pixel size of the last render (for the status line).
    pub last_size: [u32; 2],
    /// Fixed animation clock (snapshot tests: the scan lines must not depend on frame count).
    pub time_override: Option<f64>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self {
            camera: OrbitCamera::default(),
            mode: ViewMode::Shaded,
            renderer: None,
            render_state: None,
            last_size: [0, 0],
            time_override: None,
        }
    }

    /// Give the viewport the GPU. Call once (e.g. from `eframe::Frame::wgpu_render_state`).
    pub fn attach(&mut self, rs: &egui_wgpu::RenderState) {
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new(rs));
            self.render_state = Some(rs.clone());
        }
    }

    pub fn has_gpu(&self) -> bool {
        self.renderer.is_some()
    }

    /// Fit the camera to the scene's meshes (no-op on an empty scene).
    pub fn fit(&mut self, scene: &Scene) {
        if let Some((lo, hi)) = scene.bounds() {
            self.camera.fit(lo, hi);
        }
    }

    /// Draw the scene into `rect`, handle input, paint the HUD. Returns the interaction response
    /// (clicks are the caller's to interpret, e.g. for picking).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        scene: &Scene,
        pal: &Palette,
        time: f64,
    ) -> Response {
        let resp = ui.interact(
            rect,
            ui.id().with("fuide-3d-viewport"),
            Sense::click_and_drag(),
        );
        let shift = ui.input(|i| i.modifiers.shift);
        let d = resp.drag_delta();
        if resp.dragged_by(PointerButton::Primary) && !shift {
            self.camera.orbit(d.x, d.y);
        } else if resp.dragged_by(PointerButton::Secondary)
            || resp.dragged_by(PointerButton::Middle)
            || (resp.dragged_by(PointerButton::Primary) && shift)
        {
            self.camera.pan(d.x, d.y, rect.height());
        }
        if resp.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            if scroll != 0.0 {
                self.camera.zoom((scroll * 0.004).exp());
            }
            if zoom != 1.0 {
                self.camera.zoom(zoom);
            }
        }
        if resp.double_clicked() {
            self.fit(scene);
        }

        let ppp = ui.pixels_per_point();
        let size = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        self.last_size = size;
        let style = self.mode.style(pal);
        let time = self.time_override.unwrap_or(time);
        let painter = ui.painter().with_clip_rect(rect);
        match (&mut self.renderer, &self.render_state) {
            (Some(r), Some(rs)) => {
                let id = r.render(rs, scene, &self.camera, size, &style, time as f32);
                painter.image(
                    id,
                    rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                if style.scan_strength > 0.0 {
                    ui.ctx().request_repaint_after(Duration::from_millis(50));
                }
            }
            _ => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "NO GPU // VIEWPORT OFFLINE",
                    fuide::theme::mono(12.0),
                    pal.text_dim,
                );
            }
        }
        self.paint_triad(&painter, rect, pal);
        resp
    }

    /// World axes as seen by the camera, bottom-left.
    fn paint_triad(&self, painter: &egui::Painter, rect: Rect, pal: &Palette) {
        let origin = rect.left_bottom() + vec2(34.0, -34.0);
        let right = self.camera.right();
        let up = self.camera.up();
        let len = 22.0;
        for (axis, color, label) in [
            (Vec3::X, pal.danger, "X"),
            (Vec3::Y, pal.ok, "Y"),
            (Vec3::Z, pal.accent, "Z"),
        ] {
            let dir = vec2(axis.dot(right), -axis.dot(up));
            let end = origin + dir * len;
            painter.line_segment([origin, end], Stroke::new(1.5, color));
            painter.text(
                origin + dir * (len + 8.0),
                egui::Align2::CENTER_CENTER,
                label,
                fuide::theme::mono(10.0),
                color,
            );
        }
        painter.circle_filled(origin, 2.0, pal.text_dim);
    }
}
