//! State-machine tests: `apply` on the document, the evaluator polled synchronously. No UI.

use super::*;
use std::time::Instant;

fn app() -> (egui::Context, CadApp) {
    let ctx = egui::Context::default();
    let app = CadApp::with_context(&ctx, Settings::default());
    (ctx, app)
}

/// Wait for the worker to evaluate the current revision.
fn settle(app: &mut CadApp) {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while app.evaluating() {
        app.poll(0.0);
        assert!(Instant::now() < deadline, "evaluation never finished");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    app.poll(0.0);
}

fn log_has(app: &CadApp, needle: &str) -> bool {
    app.log.iter().any(|e| e.text.contains(needle))
}

#[test]
fn sample_evaluates_to_one_body() {
    let (_, mut app) = app();
    app.load_sample(0.0);
    settle(&mut app);
    let ev = app.eval.as_ref().unwrap();
    assert!(ev.errors.is_empty(), "{:?}", ev.errors);
    assert_eq!(ev.bodies.len(), 1);
    assert_eq!(ev.bodies[0].feature, 5);
    assert!(log_has(&app, "eval // rev"));
    assert_eq!(app.selected, Some(5));
}

#[test]
fn add_edit_undo_redo() {
    let (ctx, mut app) = app();
    app.apply(
        &ctx,
        Action::Add(FeatureKind::Box {
            origin: xyz(0.0, 0.0, 0.0),
            size: xyz(10.0, 10.0, 10.0),
        }),
        0.0,
    );
    assert_eq!(app.selected, Some(1));
    assert!(app.dirty);
    app.apply(
        &ctx,
        Action::SetField {
            id: 1,
            field: "size.z".into(),
            value: "20".into(),
        },
        1.0,
    );
    // a second edit of the same field inside the coalescing window shares the undo step
    app.apply(
        &ctx,
        Action::SetField {
            id: 1,
            field: "size.z".into(),
            value: "30".into(),
        },
        2.0,
    );
    settle(&mut app);
    let v = app.eval.as_ref().unwrap().bodies[0].volume;
    assert!((v - 3000.0).abs() < 1e-6, "{v}");
    app.apply(&ctx, Action::Undo, 3.0);
    assert_eq!(app.doc.get(1).unwrap().kind.fields()[5].1, "10");
    app.apply(&ctx, Action::Undo, 3.0);
    assert!(app.doc.features.is_empty());
    assert_eq!(app.selected, None);
    app.apply(&ctx, Action::Redo, 3.0);
    app.apply(&ctx, Action::Redo, 3.0);
    assert_eq!(app.doc.get(1).unwrap().kind.fields()[5].1, "30");
}

#[test]
fn boolean_is_two_clicks_and_remove_refuses_used_inputs() {
    let (ctx, mut app) = app();
    app.apply(
        &ctx,
        Action::Add(FeatureKind::Box {
            origin: xyz(0.0, 0.0, 0.0),
            size: xyz(40.0, 40.0, 10.0),
        }),
        0.0,
    );
    app.apply(
        &ctx,
        Action::Add(FeatureKind::Cylinder {
            base: xyz(20.0, 20.0, -5.0),
            axis: Axis::Z,
            radius: "5".into(),
            height: "20".into(),
        }),
        0.0,
    );
    app.apply(&ctx, Action::Select(Some(1)), 0.0);
    app.apply(&ctx, Action::BeginBoolean(BoolOp::Cut), 0.0);
    assert_eq!(app.pending, Some(BoolOp::Cut));
    // clicking the selected row again does not complete it
    app.apply(&ctx, Action::Select(Some(1)), 0.0);
    assert_eq!(app.pending, Some(BoolOp::Cut));
    app.apply(&ctx, Action::Select(Some(2)), 0.0);
    assert_eq!(app.pending, None);
    let f = app.doc.get(3).unwrap();
    assert_eq!(f.name, "CUT 3");
    assert_eq!(f.kind.inputs(), vec![1, 2]);
    assert_eq!(app.selected, Some(3));
    settle(&mut app);
    let ev = app.eval.as_ref().unwrap();
    assert_eq!(ev.bodies.len(), 1);
    assert!((ev.bodies[0].volume - (16_000.0 - std::f64::consts::PI * 250.0)).abs() < 40.0);

    app.apply(&ctx, Action::Remove(1), 0.0);
    assert!(app.doc.get(1).is_some());
    assert!(log_has(&app, "remove // BOX 1 :: failed"));
    assert!(app.dialog.is_none()); // the card opens on the next frame from error_queue
    assert_eq!(app.error_queue.len(), 1);
    app.apply(&ctx, Action::Remove(3), 0.0);
    assert!(app.doc.get(3).is_none());
    assert_eq!(app.selected, Some(2));
}

#[test]
fn params_drive_features_and_errors_are_logged_once() {
    let (ctx, mut app) = app();
    app.load_sample(0.0);
    settle(&mut app);
    app.apply(
        &ctx,
        Action::SetParam {
            name: "w".into(),
            value: "80".into(),
        },
        0.0,
    );
    settle(&mut app);
    let b = &app.eval.as_ref().unwrap().bodies[0];
    assert!((b.bbox.1[0] - 80.0).abs() < 1e-6);
    app.apply(
        &ctx,
        Action::SetParam {
            name: "w".into(),
            value: "nope".into(),
        },
        0.0,
    );
    settle(&mut app);
    let n = app
        .log
        .iter()
        .filter(|e| e.text.contains("unknown parameter 'nope'"))
        .count();
    assert_eq!(n, 1);
    assert!(app.eval.as_ref().unwrap().bodies.is_empty());
    assert!(app.apply_and_count_errors(&ctx).is_some());
}

impl CadApp {
    /// Test helper: a bad parameter name is refused with an error card, not a log line only.
    fn apply_and_count_errors(&mut self, ctx: &egui::Context) -> Option<()> {
        self.apply(
            ctx,
            Action::SetParam {
                name: "9x".into(),
                value: "1".into(),
            },
            0.0,
        );
        self.error_queue
            .iter()
            .find(|l| l.starts_with("param // 9x"))
            .map(|_| ())
    }
}

#[test]
fn save_open_and_export_round_trip() {
    let dir = std::env::temp_dir().join(format!("fuide-cad-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (ctx, mut app) = app();
    app.load_sample(0.0);
    settle(&mut app);
    let json = dir.join("bracket.cad.json");
    app.dialog = Some(OpenDialog {
        state: DialogState::File {
            op: FileOp::Save,
            path: json.display().to_string(),
            completions: Vec::new(),
        },
        closing: false,
    });
    app.apply(&ctx, Action::FileGo, 0.0);
    assert!(json.exists());
    assert!(!app.dirty);
    assert_eq!(app.path.as_deref(), Some(json.as_path()));
    // saving again over the same file asks first
    app.dialog = Some(OpenDialog {
        state: DialogState::File {
            op: FileOp::Save,
            path: json.display().to_string(),
            completions: Vec::new(),
        },
        closing: false,
    });
    app.apply(&ctx, Action::FileGo, 0.0);
    assert!(matches!(
        app.dialog.as_ref().map(|d| &d.state),
        Some(DialogState::Overwrite { .. })
    ));
    assert_eq!(app.agent_blocked(), vec!["OVERWRITE".to_string()]);
    app.apply(&ctx, Action::ConfirmDialog, 0.0);
    assert!(log_has(&app, "save // "));

    app.apply(&ctx, Action::New, 0.0);
    assert!(app.doc.features.is_empty());
    app.dialog = Some(OpenDialog {
        state: DialogState::File {
            op: FileOp::Open,
            path: json.display().to_string(),
            completions: Vec::new(),
        },
        closing: false,
    });
    app.apply(&ctx, Action::FileGo, 0.0);
    assert_eq!(app.doc.features.len(), 5);
    assert_eq!(app.doc.name, "bracket");
    settle(&mut app);

    let stl = dir.join("bracket.stl");
    app.dialog = Some(OpenDialog {
        state: DialogState::File {
            op: FileOp::ExportStl,
            path: stl.display().to_string(),
            completions: Vec::new(),
        },
        closing: false,
    });
    app.apply(&ctx, Action::FileGo, 0.0);
    let bytes = std::fs::read(&stl).unwrap();
    let tris = app.eval.as_ref().unwrap().tri_count();
    assert_eq!(bytes.len(), 84 + 50 * tris);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn agent_state_lists_features_and_bodies() {
    let (_, mut app) = app();
    app.load_sample(0.0);
    settle(&mut app);
    let s = app.agent_state();
    assert!(s.contains("#5 BRACKET [CUT] inputs #3,#4"), "{s}");
    assert!(s.contains("body #5 BRACKET: volume"));
    assert!(s.contains("params: w = 60 (60)"));
    assert!(s.contains("SELECTED"));
}
