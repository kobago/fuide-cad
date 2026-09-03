//! The viewport rendered headless through eframe + wgpu (egui_kittest), compared to
//! `tests/snapshots/*.png` (`UPDATE_SNAPSHOTS=true cargo test -p fuide-3d` to regenerate).

use egui::Vec2;
use egui_kittest::Harness;
use fuide::{theme, Palette, Panel};
use fuide_3d::{scene, LineBatch, Scene, Vec3, ViewMode, Viewport};

struct Demo {
    installed: bool,
    scene: Scene,
    viewport: Viewport,
}

impl Demo {
    fn new() -> Self {
        let mut scene = Scene::default();
        let lo = Vec3::new(-20.0, -15.0, 0.0);
        let hi = Vec3::new(20.0, 15.0, 25.0);
        let cube = scene::cube(lo, hi);
        let mut edges = LineBatch::new(1.5, [0.0, 0.9, 1.0, 1.0]);
        for tri in cube.indices.chunks_exact(6) {
            // the quad outline: 0-1, 1-2, 2-3, 3-0 of each face
            let q = [tri[0], tri[1], tri[2], tri[5]];
            for k in 0..4 {
                edges.segment(
                    cube.positions[q[k] as usize],
                    cube.positions[q[(k + 1) % 4] as usize],
                );
            }
        }
        scene.meshes.push(cube);
        scene.edges.push(edges);
        scene
            .overlay
            .push(LineBatch::grid(60.0, 10.0, [0.0, 0.9, 1.0, 0.18]));
        let mut axes = LineBatch::new(2.0, [1.0, 1.0, 1.0, 0.5]);
        axes.segment([0.0, 0.0, 0.0], [30.0, 0.0, 0.0]);
        scene.overlay.push(axes);
        scene.bump();
        let mut viewport = Viewport::new();
        viewport.fit(&scene);
        Self {
            installed: false,
            scene,
            viewport,
        }
    }
}

impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.installed {
            theme::install(ui.ctx(), Palette::cyan(), vec![]);
            self.installed = true;
            return;
        }
        if let Some(rs) = frame.wgpu_render_state() {
            self.viewport.attach(rs);
        }
        let pal = theme::palette(ui.ctx());
        let rect = ui.max_rect().shrink(20.0);
        let t = ui.input(|i| i.time);
        Panel::new("Viewport").show_rect(ui, rect, |ui| {
            let r = ui.max_rect();
            self.viewport.show(ui, r, &self.scene, &pal, t);
        });
    }
}

fn harness() -> Harness<'static, Demo> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(800.0, 600.0))
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|_cc| Demo::new());
    h.run_steps(3);
    h
}

#[test]
fn renders_with_gpu() {
    let h = harness();
    assert!(
        h.state().viewport.has_gpu(),
        "kittest should hand eframe a wgpu RenderState"
    );
    let [w, hgt] = h.state().viewport.last_size;
    assert!(w > 600 && hgt > 400, "rendered at {w}x{hgt}");
}

#[test]
fn snapshot_shaded() {
    let mut h = harness();
    h.snapshot("viewport_shaded");
}

#[test]
fn snapshot_wire_and_xray() {
    let mut h = harness();
    h.state_mut().viewport.mode = ViewMode::Wire;
    h.run_steps(2);
    h.snapshot("viewport_wire");
    h.state_mut().viewport.mode = ViewMode::Xray;
    h.run_steps(2);
    h.snapshot("viewport_xray");
}
