//! End-to-end: the real `CadApp` (its `eframe::App::ui`, every frame, wgpu viewport included)
//! driven through the accessibility tree with `egui_kittest`.

use std::time::{Duration, Instant};

use egui::{Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use super::*;
use fuide::agent::{Command, Reply};
use serde_json::json;

fn harness() -> Harness<'static, CadApp> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| {
            let mut app = CadApp::with_context(&cc.egui_ctx, Settings::default());
            if let Some(rs) = &cc.wgpu_render_state {
                app.viewport.attach(rs);
            }
            app
        });
    h.run_steps(2);
    h
}

/// Frames until the worker has evaluated the current revision.
fn settle(h: &mut Harness<'static, CadApp>) {
    h.run_steps(2); // let a queued click change the document first
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.state().evaluating() {
        h.run_steps(1);
        assert!(Instant::now() < deadline, "evaluation never finished");
        std::thread::sleep(Duration::from_millis(3));
    }
    h.run_steps(2);
}

#[test]
fn add_box_then_cylinder_then_cut_through_the_toolbar() {
    let mut h = harness();
    assert!(h.state().viewport.has_gpu());
    h.get_by_label("BOX").click();
    h.run_steps(2);
    assert_eq!(h.state().selected, Some(1));
    h.get_by_label("CYLINDER").click();
    h.run_steps(2);
    assert_eq!(h.state().selected, Some(2));
    // move the cylinder into the box through the SELECTED panel (a cylinder standing on the
    // box's corner edge is a degenerate boolean the kernel refuses)
    for (field, value) in [("BASE.X", "20"), ("BASE.Y", "15"), ("BASE.Z", "-5")] {
        h.get_by_label(field).click();
        h.run_steps(1);
        h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        h.event(egui::Event::Text(value.into()));
        h.run_steps(1);
    }
    h.key_press(Key::Escape);
    h.run_steps(1);
    assert_eq!(h.state().doc.get(2).unwrap().kind.fields()[0].1, "20");
    // select the box row, CUT, then click the cylinder row
    h.get_by_label("BOX 1").click();
    h.run_steps(2);
    assert_eq!(h.state().selected, Some(1));
    h.get_by_label("CUT").click();
    h.run_steps(2);
    assert_eq!(h.state().pending, Some(BoolOp::Cut));
    h.get_by_label("CYLINDER 2").click();
    h.run_steps(2);
    assert_eq!(h.state().pending, None);
    assert_eq!(h.state().doc.features.len(), 3);
    settle(&mut h);
    assert_eq!(h.state().eval.as_ref().unwrap().bodies.len(), 1);
}

#[test]
fn field_edit_re_evaluates_and_undo_reverts() {
    let mut h = harness();
    h.get_by_label("BOX").click();
    settle(&mut h);
    let v0 = h.state().eval.as_ref().unwrap().bodies[0].volume;
    let field = h.get_by_label("SIZE.Z");
    field.click();
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(egui::Event::Text("40".into()));
    settle(&mut h);
    let v1 = h.state().eval.as_ref().unwrap().bodies[0].volume;
    assert!((v1 - v0 * 2.0).abs() < 1e-6, "{v0} -> {v1}");
    // Escape leaves the field, Cmd+Z undoes the edit
    h.key_press(Key::Escape);
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    settle(&mut h);
    let v2 = h.state().eval.as_ref().unwrap().bodies[0].volume;
    assert!((v2 - v0).abs() < 1e-6, "{v0} -> {v2}");
}

#[test]
fn view_buttons_and_keys() {
    let mut h = harness();
    h.get_by_label("WIRE").click();
    h.run_steps(1);
    assert_eq!(h.state().viewport.mode, ViewMode::Wire);
    h.get_by_label("TOP").click();
    h.run_steps(1);
    assert!((h.state().viewport.camera.pitch.to_degrees() - 89.9).abs() < 0.01);
    h.key_press(Key::Num2);
    h.run_steps(1);
    assert!(h.state().viewport.camera.pitch.abs() < 1e-6);
}

#[test]
fn cmd_l_opens_the_in_app_open_dialog_and_cmd_o_falls_back_to_it_off_the_main_thread() {
    let mut h = harness();
    h.state_mut().native_open = false;
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    h.run_steps(2);
    assert!(matches!(
        h.state().dialog.as_ref().map(|d| &d.state),
        Some(DialogState::File {
            op: FileOp::Open,
            ..
        })
    ));
    h.key_press(Key::Escape);
    h.run_steps(30);
    assert!(h.state().dialog.is_none());
    h.key_press_modifiers(Modifiers::COMMAND, Key::L);
    h.run_steps(2);
    assert!(matches!(
        h.state().dialog.as_ref().map(|d| &d.state),
        Some(DialogState::File {
            op: FileOp::Open,
            ..
        })
    ));
}

#[test]
fn save_dialog_opens_with_cmd_s_and_escape_closes_it() {
    let mut h = harness();
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run_steps(2);
    assert!(matches!(
        h.state().dialog.as_ref().map(|d| &d.state),
        Some(DialogState::File {
            op: FileOp::Save,
            ..
        })
    ));
    h.get_by_label("PATH");
    h.key_press(Key::Escape);
    h.run_steps(30);
    assert!(h.state().dialog.is_none());
}

#[test]
fn snapshot_sample_document() {
    let mut h = harness();
    h.state_mut().viewport.time_override = Some(1.0);
    h.state_mut().load_sample(0.0);
    // wait for the kernel without advancing frames (frame count = clock = status bar text)
    let deadline = Instant::now() + Duration::from_secs(10);
    while h.state().evaluating() {
        h.state_mut().poll(0.0);
        assert!(Instant::now() < deadline, "evaluation never finished");
        std::thread::sleep(Duration::from_millis(3));
    }
    // the log carries timings: replace it with fixed lines
    h.state_mut().log.clear();
    h.state_mut()
        .push_log(0.0, "kernel online :: truck :: units mm", Level::Ok);
    h.state_mut()
        .push_log(0.0, "document // sample bracket loaded", Level::Info);
    h.state_mut()
        .push_log(0.0, "eval // rev 2 :: 1 bodies :: 160 tris", Level::Info);
    h.run_steps(2);
    h.get_by_label("FIT").click();
    h.run_steps(3);
    h.snapshot("cad_sample");
}

/// Submit an agent command and pump frames until its reply arrives.
fn drive(h: &mut Harness<'static, CadApp>, cmd: Command) -> Reply {
    let rx = h.state().agent_submit(cmd);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(r) = rx.try_recv() {
            return r;
        }
        assert!(Instant::now() < deadline, "no reply from the agent");
        h.run_steps(1);
    }
}

fn tool(
    h: &mut Harness<'static, CadApp>,
    name: &str,
    args: serde_json::Value,
) -> Result<String, String> {
    match drive(
        h,
        Command::Tool {
            name: name.into(),
            args,
        },
    ) {
        Reply::Text(t) => Ok(t),
        Reply::Error(e) => Err(e),
        Reply::Image { .. } => panic!("unexpected image"),
    }
}

#[test]
fn agent_tools_build_a_holed_plate_and_measure_it() {
    let mut h = harness();
    h.state_mut().agent_enable_detached();
    h.run_steps(1);
    let r = tool(&mut h, "set_param", json!({"name": "w", "value": "40"})).unwrap();
    assert!(r.starts_with("w = 40 (40)"), "{r}");
    let r = tool(
        &mut h,
        "add_feature",
        json!({"kind": "box", "name": "PLATE", "origin": ["0", "0", "0"], "size": ["w", "w", "10"]}),
    )
    .unwrap();
    assert!(r.starts_with("added #1 PLATE"), "{r}");
    tool(
        &mut h,
        "add_feature",
        json!({"kind": "cylinder", "base": ["w/2", "w/2", "-5"], "axis": "z", "radius": "5", "height": "20"}),
    )
    .unwrap();
    let r = tool(
        &mut h,
        "add_feature",
        json!({"kind": "cut", "a": 1, "b": 2}),
    )
    .unwrap();
    assert!(r.starts_with("added #3 CUT 3"), "{r}");
    assert!(r.contains("WIDGETS ("), "the observation follows: {r}");
    settle(&mut h);
    let r = tool(&mut h, "measure", json!({})).unwrap();
    assert!(r.starts_with("#3 CUT 3: volume 152"), "{r}");
    // structured errors, no error card for the human
    let e = tool(
        &mut h,
        "set_field",
        json!({"id": 2, "field": "depth", "value": "1"}),
    )
    .unwrap_err();
    assert!(e.contains("no field `depth`"), "{e}");
    let e = tool(&mut h, "remove_feature", json!({"id": 1})).unwrap_err();
    assert!(e.contains("used by CUT 3"), "{e}");
    assert!(h.state().dialog.is_none());
    // a field edit through the tool is undoable like a click
    tool(
        &mut h,
        "set_field",
        json!({"id": 2, "field": "radius", "value": "8"}),
    )
    .unwrap();
    assert_eq!(h.state().doc.get(2).unwrap().kind.fields()[3].1, "8");
    tool(
        &mut h,
        "set_field",
        json!({"id": 2, "field": "axis", "value": "y"}),
    )
    .unwrap();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(1);
    assert_eq!(h.state().doc.get(2).unwrap().kind.fields()[3].1, "8");
    // export is refused over an existing file while confirmations are the human's
    let dir = std::env::temp_dir().join(format!("fuide-cad-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stl = dir.join("plate.stl");
    settle(&mut h);
    tool(&mut h, "export", json!({"path": stl.display().to_string()})).unwrap();
    assert!(stl.exists());
    let e = tool(&mut h, "export", json!({"path": stl.display().to_string()})).unwrap_err();
    assert!(e.contains("reserved for the human"), "{e}");
    let doc = dir.join("plate.cad.json");
    tool(
        &mut h,
        "export",
        json!({"path": doc.display().to_string(), "format": "json"}),
    )
    .unwrap();
    assert!(!h.state().dirty);
    let r = tool(&mut h, "document", json!({})).unwrap();
    assert!(r.contains("\"kind\": \"boolean\""), "{r}");
    tool(&mut h, "open", json!({"new": true})).unwrap();
    assert!(h.state().doc.features.is_empty());
    tool(&mut h, "open", json!({"path": doc.display().to_string()})).unwrap();
    assert_eq!(h.state().doc.features.len(), 3);
    let r = tool(
        &mut h,
        "view",
        json!({"preset": "top", "mode": "wire", "fit": true}),
    )
    .unwrap();
    assert!(r.starts_with("view :: preset top, mode wire, fit"), "{r}");
    assert_eq!(h.state().viewport.mode, ViewMode::Wire);
    std::fs::remove_dir_all(&dir).unwrap();
}
