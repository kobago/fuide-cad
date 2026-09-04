//! FUIDE CAD — the egui application: a document (parameters + feature list) edited from the
//! feature panel, the selected-feature panel and the MCP agent; evaluated on a worker thread
//! through the truck kernel; the result bodies drawn by `fuide-3d` in the centre.

#[cfg(test)]
mod e2e;
#[cfg(test)]
mod tests;
pub mod tools;
mod ui;

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use egui::{Key, Ui};
use fuide::table::TableState;
use fuide::{theme, PaletteKind, Settings, SettingsWindow};
use fuide_3d::{LineBatch, MeshData, Preset, Scene, ViewMode, Viewport};

use crate::doc::{fmt_num, xyz, Axis, BoolOp, Document, FeatureId, FeatureKind};
use crate::eval::{self, Evaluation};
use crate::filepicker::FilePicker;
use crate::mesh;

pub(crate) const APP_ID: &str = "cad";
const APP_NAME: &str = "FUIDE CAD";
const LEFT_W: f32 = 280.0;
const RIGHT_W: f32 = 280.0;
const GAP: f32 = 14.0;
/// Two rows: modelling verbs, then view controls.
const TOOLBAR_H: f32 = 32.0;
const TOOLBAR_ROWS: f32 = 2.0;
const LOG_H: f32 = 110.0;
const LOG_MIN: f32 = 60.0;
const LOG_CLOSED: f32 = 26.0;
const BODY_MIN: f32 = 420.0;
const PARAMS_H: f32 = 190.0;
const UNDO_DEPTH: usize = 100;
/// Consecutive edits of the same field within this window share one undo step.
const UNDO_COALESCE_SECS: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Level {
    Info,
    Ok,
    Warn,
    Danger,
}

#[derive(Clone, Debug)]
pub(crate) struct Event {
    pub(crate) time: String,
    pub(crate) text: String,
    pub(crate) level: Level,
}

/// Which file dialog is open and what it will do with the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOp {
    Open,
    Save,
    ExportStl,
}

impl FileOp {
    pub(crate) fn verb(self) -> &'static str {
        match self {
            FileOp::Open => "OPEN",
            FileOp::Save => "SAVE",
            FileOp::ExportStl => "EXPORT",
        }
    }
    fn title(self) -> &'static str {
        match self {
            FileOp::Open => "Open document",
            FileOp::Save => "Save document",
            FileOp::ExportStl => "Export STL",
        }
    }
}

pub(crate) enum DialogState {
    /// Path input (`~` and relative paths, Tab completion).
    File {
        op: FileOp,
        path: String,
        completions: Vec<String>,
    },
    /// The path exists: overwrite? (reserved for the human unless the agent may confirm)
    Overwrite {
        op: FileOp,
        path: PathBuf,
    },
    Error(String),
}

pub(crate) struct OpenDialog {
    pub(crate) state: DialogState,
    pub(crate) closing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Select(Option<FeatureId>),
    Add(FeatureKind),
    /// Start a boolean: the selected feature is `a`, the next clicked row is `b`.
    BeginBoolean(BoolOp),
    CancelPending,
    Remove(FeatureId),
    ToggleSuppress(FeatureId),
    SetField {
        id: FeatureId,
        field: String,
        value: String,
    },
    Rename {
        id: FeatureId,
        name: String,
    },
    SetParam {
        name: String,
        value: String,
    },
    RemoveParam(String),
    Undo,
    Redo,
    Fit,
    Preset(Preset),
    Mode(ViewMode),
    New,
    /// Open the path dialog for `op`.
    File(FileOp),
    /// Cmd+O: the macOS open panel (falls back to the in-app dialog off the main thread).
    OpenFiles,
    /// Run `op` on the dialog's path (asks before overwriting).
    FileGo,
    ConfirmDialog,
    CloseDialog,
    Palette(PaletteKind),
    OpenSettings,
}

// ---- evaluation worker ----------------------------------------------------------------------

struct Worker {
    tx: Sender<(Document, u64)>,
    rx: Receiver<Evaluation>,
    busy: Arc<AtomicBool>,
}

impl Worker {
    fn spawn(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, jobs) = channel::<(Document, u64)>();
        let (results, rx) = channel::<Evaluation>();
        let busy = Arc::new(AtomicBool::new(false));
        let flag = busy.clone();
        std::thread::Builder::new()
            .name("cad-eval".into())
            .spawn(move || {
                while let Ok(mut job) = jobs.recv() {
                    // only the newest document matters
                    while let Ok(newer) = jobs.try_recv() {
                        job = newer;
                    }
                    flag.store(true, Ordering::SeqCst);
                    let ev = eval::evaluate(&job.0, mesh::MESH_TOL, job.1);
                    flag.store(false, Ordering::SeqCst);
                    if results.send(ev).is_err() {
                        break;
                    }
                    wake();
                }
            })
            .expect("spawn evaluator");
        Self { tx, rx, busy }
    }
}

// ---- the app --------------------------------------------------------------------------------

pub struct CadApp {
    pub(crate) doc: Document,
    /// Bumped on every document change; evaluations carry the revision they were made from.
    pub(crate) revision: u64,
    pub(crate) path: Option<PathBuf>,
    /// Unsaved changes since the last save / open / new.
    pub(crate) dirty: bool,
    saved_revision: u64,
    pub(crate) eval: Option<Evaluation>,
    worker: Worker,
    eval_sent: u64,
    pub(crate) selected: Option<FeatureId>,
    pub(crate) pending: Option<BoolOp>,
    pub(crate) table: TableState,
    undo: Vec<Document>,
    redo: Vec<Document>,
    last_edit: Option<(FeatureId, String, f64)>,
    pub(crate) viewport: Viewport,
    scene: Scene,
    scene_for: (u64, Option<FeatureId>),
    /// The name field of the selected feature while it is being edited.
    name_edit: Option<(FeatureId, String)>,
    /// Draft for a new parameter: name, value.
    pub(crate) new_param: (String, String),
    pub(crate) log: Vec<Event>,
    pub(crate) dialog: Option<OpenDialog>,
    error_queue: VecDeque<String>,
    settings: Settings,
    settings_win: SettingsWindow,
    settings_path: Option<PathBuf>,
    log_h: f32,
    devshot: fuide::devshot::DevShot,
    agent: fuide::Agent,
    /// `FUIDE_DEV_CAMERA` set: do not auto-fit on the first evaluation.
    camera_locked: bool,
    picker: FilePicker,
    /// Use the macOS open panel for Cmd+O (tests turn it off).
    pub(crate) native_open: bool,
}

impl CadApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load(APP_ID).unwrap_or_else(|| Settings::new(PaletteKind::Cyan));
        let mut app = Self::with_context(&cc.egui_ctx, settings);
        if let Some(path) = Settings::path(APP_ID) {
            app.settings_path = Some(path);
        }
        if let Some(rs) = &cc.wgpu_render_state {
            app.viewport.attach(rs);
        }
        // `fuide-cad FILE`: open a document (the launcher script passes an absolute path)
        if let Some(arg) = std::env::args().nth(1).filter(|a| !a.starts_with("--")) {
            app.run_file_op(FileOp::Open, PathBuf::from(arg), 0.0);
        }
        app
    }

    pub fn with_context(ctx: &egui::Context, settings: Settings) -> Self {
        theme::install(ctx, settings.palette.palette(), theme::macos_cjk_fallback());
        settings.apply(ctx);
        let wake_ctx = ctx.clone();
        let worker = Worker::spawn(move || wake_ctx.request_repaint());
        let mut app = Self {
            doc: Document::default(),
            revision: 1,
            path: None,
            dirty: false,
            saved_revision: 1,
            eval: None,
            worker,
            eval_sent: 0,
            selected: None,
            pending: None,
            table: TableState::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            viewport: Viewport::new(),
            scene: Scene::default(),
            scene_for: (0, None),
            name_edit: None,
            new_param: (String::new(), String::new()),
            log: Vec::new(),
            dialog: None,
            error_queue: VecDeque::new(),
            log_h: settings.log_height.unwrap_or(LOG_H),
            settings,
            settings_win: SettingsWindow::default(),
            settings_path: None,
            devshot: fuide::devshot::DevShot::from_env(),
            agent: fuide::Agent::new(APP_ID, APP_NAME),
            camera_locked: false,
            picker: FilePicker::default(),
            native_open: true,
        };
        app.agent.set_tools(tools::tools());
        app.push_log(
            0.0,
            "kernel online :: manifold (mesh) + truck (step) :: units mm",
            Level::Ok,
        );
        app.agent.set_enabled(ctx, app.settings.agent);
        if app.settings.agent {
            app.push_log(
                0.0,
                "agent // interface on :: waiting for a client",
                Level::Warn,
            );
        }
        if std::env::var_os("FUIDE_DEV_SAMPLE").is_some() {
            app.load_sample(0.0);
        }
        if std::env::var_os("FUIDE_DEV_SETTINGS").is_some() {
            app.settings_win.open();
        }
        // FUIDE_DEV_CAMERA="yaw,pitch,dist,tx,ty,tz" (degrees, mm): fixed view for screenshots
        if let Ok(spec) = std::env::var("FUIDE_DEV_CAMERA") {
            let v: Vec<f32> = spec
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            if v.len() == 6 {
                let cam = &mut app.viewport.camera;
                cam.yaw = v[0].to_radians();
                cam.pitch = v[1].to_radians();
                cam.dist = v[2];
                cam.target = fuide_3d::Vec3::new(v[3], v[4], v[5]);
                app.camera_locked = true;
            }
        }
        if let Ok(text) = std::env::var("FUIDE_DEV_LOG") {
            app.push_log(0.0, text, Level::Danger);
        }
        if let Ok(kind) = std::env::var("FUIDE_DEV_DIALOG") {
            app.dev_dialog(&kind);
        }
        app.request_eval();
        app
    }

    /// The plate-with-hole document used by screenshots and tests.
    pub(crate) fn load_sample(&mut self, t: f64) {
        let mut d = Document::new("bracket");
        d.set_param("w", "60").unwrap();
        d.set_param("d", "40").unwrap();
        d.set_param("hole", "4").unwrap();
        let plate = d.add(
            FeatureKind::Box {
                origin: xyz(0.0, 0.0, 0.0),
                size: ["w".into(), "d".into(), "6".into()],
            },
            Some("PLATE"),
        );
        let wall = d.add(
            FeatureKind::Box {
                origin: xyz(0.0, 0.0, 0.0),
                size: ["6".into(), "d".into(), "30".into()],
            },
            Some("WALL"),
        );
        let body = d.add(
            FeatureKind::Boolean {
                op: BoolOp::Union,
                a: plate,
                b: wall,
            },
            Some("BODY"),
        );
        let hole = d.add(
            FeatureKind::Cylinder {
                base: ["w - 12".into(), "d / 2".into(), "-1".into()],
                axis: Axis::Z,
                radius: "hole".into(),
                height: "8".into(),
            },
            Some("MOUNT HOLE"),
        );
        d.add(
            FeatureKind::Boolean {
                op: BoolOp::Cut,
                a: body,
                b: hole,
            },
            Some("BRACKET"),
        );
        self.doc = d;
        self.path = None;
        self.undo.clear();
        self.redo.clear();
        self.selected = Some(5);
        self.touch(t, false);
        self.saved_revision = self.revision;
        self.dirty = false;
        self.push_log(t, "document // sample bracket loaded", Level::Info);
    }

    fn dev_dialog(&mut self, kind: &str) {
        let state = match kind {
            "open" => DialogState::File {
                op: FileOp::Open,
                path: "~/Documents/".into(),
                completions: Vec::new(),
            },
            "save" => DialogState::File {
                op: FileOp::Save,
                path: "~/Documents/bracket.cad.json".into(),
                completions: Vec::new(),
            },
            "overwrite" => DialogState::Overwrite {
                op: FileOp::Save,
                path: PathBuf::from("/tmp/bracket.cad.json"),
            },
            _ => DialogState::Error("export // bracket.stl".into()),
        };
        self.dialog = Some(OpenDialog {
            state,
            closing: false,
        });
    }

    pub(crate) fn push_log(&mut self, t: f64, text: impl Into<String>, level: Level) {
        self.log.push(Event {
            time: format!("[{}]", fuide::fmt::uptime(t)),
            text: text.into(),
            level,
        });
        if self.log.len() > 300 {
            self.log.drain(..100);
        }
    }

    fn fail(&mut self, t: f64, line: impl Into<String>, detail: &str) {
        let line = line.into();
        self.push_log(t, format!("{line} :: failed :: {detail}"), Level::Danger);
        self.error_queue.push_back(line);
    }

    fn save_settings(&mut self, t: f64) {
        if let Some(path) = self.settings_path.clone() {
            if let Err(e) = self.settings.save_to(&path) {
                self.push_log(t, format!("settings // save failed: {e}"), Level::Danger);
            }
        }
    }

    fn settings_changed(&mut self, t: f64) {
        let s = &self.settings;
        self.push_log(
            t,
            format!(
                "settings // palette {} :: {} :: {} :: {} :: agent {}{}",
                s.palette.name(),
                if s.chamfer { "chamfer" } else { "square" },
                if s.compact { "compact" } else { "normal" },
                if s.transparent {
                    "translucent"
                } else {
                    "opaque"
                },
                if s.agent { "on" } else { "off" },
                if s.agent && s.agent_confirm {
                    " (may confirm)"
                } else {
                    ""
                },
            ),
            Level::Warn,
        );
        self.save_settings(t);
    }

    // ---- document changes --------------------------------------------------------------------

    /// Record an undo step (unless coalesced) before a change.
    fn snapshot(&mut self, t: f64, coalesce: Option<(FeatureId, &str)>) {
        if let Some((id, field)) = coalesce {
            if let Some((lid, lfield, at)) = &self.last_edit {
                if *lid == id && lfield == field && t - at < UNDO_COALESCE_SECS {
                    self.last_edit = Some((id, field.to_string(), t));
                    return;
                }
            }
            self.last_edit = Some((id, field.to_string(), t));
        } else {
            self.last_edit = None;
        }
        self.undo.push(self.doc.clone());
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    #[cfg(test)]
    pub(crate) fn agent_submit(
        &self,
        cmd: fuide::agent::Command,
    ) -> std::sync::mpsc::Receiver<fuide::agent::Reply> {
        self.agent.submit(cmd)
    }

    #[cfg(test)]
    pub(crate) fn agent_enable_detached(&mut self) {
        self.agent.enable_without_server();
    }

    pub(crate) fn snapshot_pub(&mut self, t: f64) {
        self.snapshot(t, None);
    }

    pub(crate) fn touch_pub(&mut self, t: f64) {
        self.touch(t, true);
    }

    pub(crate) fn run_file_op_pub(&mut self, op: FileOp, path: PathBuf, t: f64) {
        self.run_file_op(op, path, t);
    }

    /// The document changed: new revision, re-evaluate.
    fn touch(&mut self, _t: f64, dirty: bool) {
        self.revision += 1;
        if dirty {
            self.dirty = true;
        }
        if let Some(id) = self.selected {
            if self.doc.get(id).is_none() {
                self.selected = None;
            }
        }
        self.table.selected = self.selected.and_then(|id| self.doc.index_of(id));
        self.request_eval();
    }

    fn request_eval(&mut self) {
        if self.eval_sent != self.revision {
            self.eval_sent = self.revision;
            let _ = self.worker.tx.send((self.doc.clone(), self.revision));
        }
    }

    pub(crate) fn evaluating(&self) -> bool {
        self.worker.busy.load(Ordering::SeqCst)
            || self.eval.as_ref().map(|e| e.revision) != Some(self.revision)
    }

    /// Take finished evaluations from the worker, and the file panel's answer.
    pub(crate) fn poll(&mut self, t: f64) -> bool {
        if let Some(answer) = self.picker.poll() {
            match answer {
                Some(path) => self.run_file_op(FileOp::Open, path, t),
                None => self.push_log(t, "open // cancelled", Level::Info),
            }
        }
        let mut got = false;
        while let Ok(ev) = self.worker.rx.try_recv() {
            self.ingest(ev, t);
            got = true;
        }
        got
    }

    pub(crate) fn ingest(&mut self, ev: Evaluation, t: f64) {
        if self
            .eval
            .as_ref()
            .is_some_and(|old| old.revision > ev.revision)
        {
            return; // stale
        }
        let prev_errors: Vec<String> = self
            .eval
            .as_ref()
            .map(|e| e.errors.iter().map(|x| x.message.clone()).collect())
            .unwrap_or_default();
        for e in &ev.errors {
            if !prev_errors.contains(&e.message) {
                let who = match self.doc.get(e.feature) {
                    Some(f) => format!("{} (#{})", f.name, f.id),
                    None => "document".into(),
                };
                self.push_log(t, format!("{who} :: {}", e.message), Level::Danger);
            }
        }
        let prev_notes: Vec<String> = self
            .eval
            .as_ref()
            .map(|e| e.notes.clone())
            .unwrap_or_default();
        for n in &ev.notes {
            if !prev_notes.contains(n) {
                self.push_log(t, format!("kernel // {n}"), Level::Warn);
            }
        }
        let first = self.eval.is_none();
        self.push_log(
            t,
            format!(
                "eval // rev {} :: {} bodies :: {} tris :: {} ms{}",
                ev.revision,
                ev.bodies.len(),
                ev.tri_count(),
                ev.elapsed.as_millis(),
                if ev.errors.is_empty() {
                    String::new()
                } else {
                    format!(" :: {} errors", ev.errors.len())
                }
            ),
            if ev.errors.is_empty() {
                Level::Info
            } else {
                Level::Warn
            },
        );
        self.eval = Some(ev);
        if first || self.scene.meshes.is_empty() {
            self.rebuild_scene();
            if !self.camera_locked {
                self.viewport.fit(&self.scene);
            }
        }
    }

    /// Rebuild the 3D scene from the evaluation when it or the selection changed.
    pub(crate) fn sync_scene(&mut self) {
        let key = (
            self.eval.as_ref().map(|e| e.revision).unwrap_or(0),
            self.selected,
        );
        if key != self.scene_for {
            self.rebuild_scene();
        }
    }

    fn rebuild_scene(&mut self) {
        let pal = self.settings.palette.palette();
        let rgba = |c: egui::Color32, a: f32| {
            [
                c.r() as f32 / 255.0,
                c.g() as f32 / 255.0,
                c.b() as f32 / 255.0,
                a,
            ]
        };
        self.scene.clear();
        let mut extent = 100.0f32;
        if let Some(ev) = &self.eval {
            if let Some((lo, hi)) = ev.bbox() {
                let far = lo
                    .iter()
                    .chain(hi.iter())
                    .fold(0.0f64, |m, v| m.max(v.abs())) as f32;
                extent = ((far * 1.5 / 10.0).ceil() * 10.0).clamp(100.0, 100_000.0);
            }
            for b in &ev.bodies {
                let selected = Some(b.feature) == self.selected;
                self.scene.meshes.push(MeshData {
                    positions: b.mesh.positions.clone(),
                    normals: b.mesh.normals.clone(),
                    indices: b.mesh.indices.clone(),
                    color: if selected {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.75, 0.75, 0.75, 0.9]
                    },
                });
                let mut edges = LineBatch::new(
                    if selected { 1.6 } else { 1.2 },
                    if selected {
                        rgba(pal.warn, 1.0)
                    } else {
                        rgba(pal.accent, 1.0)
                    },
                );
                for [p, q] in &b.mesh.edges {
                    edges.segment(*p, *q);
                }
                self.scene.edges.push(edges);
            }
        }
        let step = if extent > 1000.0 { 100.0 } else { 10.0 };
        self.scene
            .overlay
            .push(LineBatch::grid(extent, step, rgba(pal.accent_dim, 0.22)));
        let mut axes = LineBatch::new(1.5, [0.0; 4]);
        axes.color = rgba(pal.danger, 0.7);
        axes.segment([0.0; 3], [extent * 0.3, 0.0, 0.0]);
        self.scene.overlay.push(axes);
        let mut y = LineBatch::new(1.5, rgba(pal.ok, 0.7));
        y.segment([0.0; 3], [0.0, extent * 0.3, 0.0]);
        self.scene.overlay.push(y);
        let mut z = LineBatch::new(1.5, rgba(pal.accent, 0.7));
        z.segment([0.0; 3], [0.0, 0.0, extent * 0.3]);
        self.scene.overlay.push(z);
        self.scene.bump();
        self.scene_for = (
            self.eval.as_ref().map(|e| e.revision).unwrap_or(0),
            self.selected,
        );
    }

    pub(crate) fn selected_name(&self) -> Option<String> {
        self.selected
            .and_then(|id| self.doc.get(id))
            .map(|f| f.name.clone())
    }

    // ---- files -------------------------------------------------------------------------------

    fn default_path(&self, op: FileOp) -> String {
        if let (FileOp::Save, Some(p)) = (op, &self.path) {
            return p.display().to_string();
        }
        let stem = if self.doc.name.trim().is_empty() {
            "untitled".to_string()
        } else {
            self.doc.name.trim().to_lowercase().replace(' ', "-")
        };
        let dir = self
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(|d| d.display().to_string())
            .unwrap_or_else(|| "~/Documents".into());
        match op {
            FileOp::Open => format!("{dir}/"),
            FileOp::Save => format!("{dir}/{stem}.cad.json"),
            FileOp::ExportStl => format!("{dir}/{stem}.stl"),
        }
    }

    fn cwd() -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    }

    fn run_file_op(&mut self, op: FileOp, path: PathBuf, t: f64) {
        let shown = path.display().to_string();
        match op {
            FileOp::Open => match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| Document::from_json(&text))
            {
                Ok(doc) => {
                    self.doc = doc;
                    self.path = Some(path);
                    self.undo.clear();
                    self.redo.clear();
                    self.selected = self.doc.features.last().map(|f| f.id);
                    self.touch(t, false);
                    self.saved_revision = self.revision;
                    self.dirty = false;
                    self.scene.clear();
                    self.push_log(
                        t,
                        format!(
                            "open // {shown} :: {} features :: {} params",
                            self.doc.features.len(),
                            self.doc.params.len()
                        ),
                        Level::Ok,
                    );
                }
                Err(e) => self.fail(t, format!("open // {shown}"), &e),
            },
            FileOp::Save => {
                let text = self.doc.to_json();
                match std::fs::write(&path, text) {
                    Ok(()) => {
                        self.path = Some(path);
                        self.saved_revision = self.revision;
                        self.dirty = false;
                        self.push_log(t, format!("save // {shown}"), Level::Ok);
                    }
                    Err(e) => self.fail(t, format!("save // {shown}"), &e.to_string()),
                }
            }
            FileOp::ExportStl => {
                let Some(ev) = &self.eval else {
                    self.fail(t, format!("export // {shown}"), "nothing evaluated yet");
                    return;
                };
                if ev.bodies.is_empty() {
                    self.fail(t, format!("export // {shown}"), "no result bodies");
                    return;
                }
                let meshes: Vec<&mesh::MeshOut> = ev.bodies.iter().map(|b| &b.mesh).collect();
                let tris = ev.tri_count();
                let result = std::fs::File::create(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        let mut w = std::io::BufWriter::new(f);
                        mesh::write_stl(&meshes, &mut w)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    });
                match result {
                    Ok(()) => self.push_log(
                        t,
                        format!(
                            "export // {shown} :: {} bodies :: {tris} triangles",
                            ev.bodies.len()
                        ),
                        Level::Ok,
                    ),
                    Err(e) => self.fail(t, format!("export // {shown}"), &e),
                }
            }
        }
    }

    // ---- actions -----------------------------------------------------------------------------

    pub(crate) fn apply(&mut self, ctx: &egui::Context, action: Action, t: f64) {
        match action {
            Action::Select(id) => {
                if let (Some(op), Some(a), Some(b)) = (self.pending, self.selected, id) {
                    if a != b {
                        self.pending = None;
                        self.snapshot(t, None);
                        let name = format!("{} {}", op.label(), self.doc.next_id());
                        let nid = self.doc.add(FeatureKind::Boolean { op, a, b }, Some(&name));
                        self.selected = Some(nid);
                        self.touch(t, true);
                        self.push_log(
                            t,
                            format!(
                                "feature // {name} :: #{a} {} #{b}",
                                op.label().to_lowercase()
                            ),
                            Level::Info,
                        );
                        return;
                    }
                }
                self.selected = id.filter(|i| self.doc.get(*i).is_some());
                self.table.selected = self.selected.and_then(|id| self.doc.index_of(id));
                self.table.scroll_to_selected = true;
                self.name_edit = None;
            }
            Action::Add(kind) => {
                self.snapshot(t, None);
                let label = kind.label();
                let id = self.doc.add(kind, None);
                self.selected = Some(id);
                self.touch(t, true);
                self.push_log(t, format!("feature // {label} {id} added"), Level::Info);
            }
            Action::BeginBoolean(op) => {
                if self.selected.is_some() {
                    self.pending = Some(op);
                    self.push_log(
                        t,
                        format!(
                            "{} // pick the tool body in the feature list",
                            op.label().to_lowercase()
                        ),
                        Level::Warn,
                    );
                }
            }
            Action::CancelPending => self.pending = None,
            Action::Remove(id) => {
                let name = self.doc.get(id).map(|f| f.name.clone());
                let mut trial = self.doc.clone();
                match trial.remove(id) {
                    Ok(_) => {
                        self.snapshot(t, None);
                        self.doc = trial;
                        if self.selected == Some(id) {
                            self.selected = self.doc.features.last().map(|f| f.id);
                        }
                        self.touch(t, true);
                        self.push_log(
                            t,
                            format!("feature // {} removed", name.unwrap_or_default()),
                            Level::Warn,
                        );
                    }
                    Err(e) => self.fail(t, format!("remove // {}", name.unwrap_or_default()), &e),
                }
            }
            Action::ToggleSuppress(id) => {
                if let Some(f) = self.doc.get(id) {
                    let (name, on) = (f.name.clone(), !f.suppressed);
                    self.snapshot(t, None);
                    self.doc.get_mut(id).unwrap().suppressed = on;
                    self.touch(t, true);
                    self.push_log(
                        t,
                        format!(
                            "feature // {name} {}",
                            if on { "suppressed" } else { "unsuppressed" }
                        ),
                        Level::Info,
                    );
                }
            }
            Action::SetField { id, field, value } => {
                let Some(f) = self.doc.get(id) else {
                    return;
                };
                if f.kind
                    .fields()
                    .iter()
                    .any(|(n, v)| *n == field && *v == value.trim())
                {
                    return;
                }
                if field != "axis" && !f.kind.fields().iter().any(|(n, _)| *n == field) {
                    self.fail(
                        t,
                        format!("set // {field}"),
                        &format!("{} has no field '{field}'", f.kind.label()),
                    );
                    return;
                }
                self.snapshot(t, Some((id, &field)));
                let f = self.doc.get_mut(id).unwrap();
                let result = if field == "axis" {
                    tools::set_axis_pub(&mut f.kind, &value)
                } else {
                    f.kind.set_field(&field, &value)
                };
                match result {
                    Ok(()) => self.touch(t, true),
                    Err(e) => {
                        self.undo.pop();
                        self.fail(t, format!("set // {field}"), &e);
                    }
                }
            }
            Action::Rename { id, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                if let Some(f) = self.doc.get(id) {
                    if f.name != name {
                        self.snapshot(t, None);
                        self.doc.get_mut(id).unwrap().name = name;
                        self.touch(t, true);
                    }
                }
            }
            Action::SetParam { name, value } => {
                let unchanged = self
                    .doc
                    .param(name.trim())
                    .is_some_and(|p| p.value == value.trim());
                if unchanged {
                    return;
                }
                let is_new = self.doc.param(name.trim()).is_none();
                self.snapshot(t, Some((0, &name)));
                match self.doc.set_param(&name, &value) {
                    Ok(()) => {
                        if is_new {
                            self.new_param = (String::new(), String::new());
                            self.push_log(
                                t,
                                format!("param // {} = {}", name.trim(), value.trim()),
                                Level::Info,
                            );
                        }
                        self.touch(t, true);
                    }
                    Err(e) => {
                        self.undo.pop();
                        self.fail(t, format!("param // {}", name.trim()), &e);
                    }
                }
            }
            Action::RemoveParam(name) => {
                self.snapshot(t, None);
                if self.doc.remove_param(&name) {
                    self.touch(t, true);
                    self.push_log(t, format!("param // {name} removed"), Level::Warn);
                } else {
                    self.undo.pop();
                }
            }
            Action::Undo => {
                if let Some(d) = self.undo.pop() {
                    self.redo.push(std::mem::replace(&mut self.doc, d));
                    self.last_edit = None;
                    self.touch(t, true);
                    self.push_log(t, "undo", Level::Info);
                }
            }
            Action::Redo => {
                if let Some(d) = self.redo.pop() {
                    self.undo.push(std::mem::replace(&mut self.doc, d));
                    self.last_edit = None;
                    self.touch(t, true);
                    self.push_log(t, "redo", Level::Info);
                }
            }
            Action::Fit => {
                self.sync_scene();
                self.viewport.fit(&self.scene);
            }
            Action::Preset(p) => {
                self.viewport.camera.set_preset(p);
                self.push_log(
                    t,
                    format!("view // {}", p.label().to_lowercase()),
                    Level::Info,
                );
            }
            Action::Mode(m) => {
                self.viewport.mode = m;
                self.push_log(
                    t,
                    format!("view // {}", m.label().to_lowercase()),
                    Level::Info,
                );
            }
            Action::New => {
                self.snapshot(t, None);
                self.doc = Document::default();
                self.path = None;
                self.selected = None;
                self.pending = None;
                self.touch(t, false);
                self.saved_revision = self.revision;
                self.dirty = false;
                self.scene.clear();
                self.push_log(t, "document // new", Level::Info);
            }
            Action::File(op) => {
                if op == FileOp::ExportStl && self.eval.as_ref().is_none_or(|e| e.bodies.is_empty())
                {
                    self.fail(t, "export // stl", "no result bodies to export");
                    return;
                }
                let path = self.default_path(op);
                self.dialog = Some(OpenDialog {
                    state: DialogState::File {
                        op,
                        path,
                        completions: Vec::new(),
                    },
                    closing: false,
                });
            }
            Action::OpenFiles => {
                let start = self
                    .path
                    .as_ref()
                    .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                    .or_else(|| {
                        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents"))
                    });
                if self.native_open && self.picker.show(start.as_deref()) {
                    self.push_log(t, "open // macOS file dialog", Level::Info);
                } else {
                    self.apply(ctx, Action::File(FileOp::Open), t);
                }
            }
            Action::FileGo => {
                let Some(OpenDialog {
                    state: DialogState::File { op, path, .. },
                    closing: false,
                }) = &self.dialog
                else {
                    return;
                };
                let (op, text) = (*op, path.clone());
                let path = fuide::pathinput::expand(&text, &Self::cwd());
                if op != FileOp::Open && path.exists() {
                    self.dialog = Some(OpenDialog {
                        state: DialogState::Overwrite { op, path },
                        closing: false,
                    });
                    return;
                }
                if let Some(d) = &mut self.dialog {
                    d.closing = true;
                }
                self.run_file_op(op, path, t);
            }
            Action::ConfirmDialog => {
                let Some(OpenDialog {
                    state: DialogState::Overwrite { op, path },
                    closing: false,
                }) = &self.dialog
                else {
                    return;
                };
                let (op, path) = (*op, path.clone());
                if let Some(d) = &mut self.dialog {
                    d.closing = true;
                }
                self.run_file_op(op, path, t);
            }
            Action::CloseDialog => {
                if let Some(d) = &mut self.dialog {
                    d.closing = true;
                }
            }
            Action::Palette(kind) => {
                self.settings.palette = kind;
                self.settings.apply(ctx);
                self.settings_changed(t);
                self.scene_for = (0, None); // colours changed
            }
            Action::OpenSettings => self.settings_win.open(),
        }
    }

    pub(crate) fn handle_keys(&self, ui: &Ui, actions: &mut Vec<Action>) {
        if self.dialog.is_some() {
            return;
        }
        let focused = ui.memory(|m| m.focused());
        let typing = focused.is_some_and(|id| egui::TextEdit::load_state(ui.ctx(), id).is_some());
        ui.input(|i| {
            let cmd = i.modifiers.command;
            if cmd && i.key_pressed(Key::Comma) {
                actions.push(Action::OpenSettings);
            }
            if cmd && i.key_pressed(Key::Z) {
                actions.push(if i.modifiers.shift {
                    Action::Redo
                } else {
                    Action::Undo
                });
            }
            if cmd && i.key_pressed(Key::S) {
                actions.push(Action::File(FileOp::Save));
            }
            if cmd && i.key_pressed(Key::O) {
                actions.push(Action::OpenFiles);
            }
            if cmd && i.key_pressed(Key::L) {
                actions.push(Action::File(FileOp::Open));
            }
            if cmd && i.key_pressed(Key::E) {
                actions.push(Action::File(FileOp::ExportStl));
            }
            if cmd && i.key_pressed(Key::N) {
                actions.push(Action::New);
            }
            if cmd && i.key_pressed(Key::Backspace) {
                if let Some(id) = self.selected {
                    actions.push(Action::Remove(id));
                }
            }
            if typing || cmd {
                return;
            }
            if i.key_pressed(Key::Escape) {
                if self.pending.is_some() {
                    actions.push(Action::CancelPending);
                } else if self.selected.is_some() {
                    actions.push(Action::Select(None));
                }
            }
            for (key, preset) in [
                (Key::Num1, Preset::Iso),
                (Key::Num2, Preset::Front),
                (Key::Num3, Preset::Top),
                (Key::Num4, Preset::Right),
            ] {
                if i.key_pressed(key) {
                    actions.push(Action::Preset(preset));
                }
            }
            if i.key_pressed(Key::F) {
                actions.push(Action::Fit);
            }
            if i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp) {
                let dir: isize = if i.key_pressed(Key::ArrowDown) { 1 } else { -1 };
                let n = self.doc.features.len() as isize;
                if n > 0 {
                    let cur = self.selected.and_then(|id| self.doc.index_of(id));
                    let next = match cur {
                        Some(c) => (c as isize + dir).clamp(0, n - 1) as usize,
                        None => 0,
                    };
                    actions.push(Action::Select(Some(self.doc.features[next].id)));
                }
            }
        });
    }

    // ---- agent -------------------------------------------------------------------------------

    pub(crate) fn agent_blocked(&self) -> Vec<String> {
        match &self.dialog {
            Some(OpenDialog {
                state: DialogState::Overwrite { .. },
                closing: false,
            }) if !self.settings.agent_confirm => vec!["OVERWRITE".into()],
            _ => Vec::new(),
        }
    }

    pub(crate) fn agent_state(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "document: {} :: {} :: {} params :: {} features :: view {} {}",
            self.doc.name,
            match &self.path {
                Some(p) => p.display().to_string(),
                None => "unsaved".into(),
            },
            self.doc.params.len(),
            self.doc.features.len(),
            self.viewport.mode.label().to_lowercase(),
            if self.dirty { ":: modified" } else { "" }
        );
        s.push_str("units mm; every value is an expression (numbers, + - * / ( ), parameter names, sqrt/sin/cos/min/max)\n");
        if !self.doc.params.is_empty() {
            let (vals, _) = eval::resolve_params(&self.doc);
            let list: Vec<String> = self
                .doc
                .params
                .iter()
                .map(|p| match vals.get(&p.name) {
                    Some(v) => format!("{} = {} ({})", p.name, p.value, fmt_num(*v)),
                    None => format!("{} = {} (error)", p.name, p.value),
                })
                .collect();
            let _ = writeln!(s, "params: {}", list.join(", "));
        }
        let _ = writeln!(
            s,
            "features (rows in the FEATURES list are named by feature; click a row to select):"
        );
        for f in &self.doc.features {
            let status = if f.suppressed {
                "suppressed".to_string()
            } else if let Some(e) = self.eval.as_ref().and_then(|e| e.error_for(f.id)) {
                format!("ERROR {e}")
            } else if self.doc.is_result(f.id) {
                "result body".into()
            } else {
                "consumed".into()
            };
            let fields: Vec<String> = f
                .kind
                .fields()
                .iter()
                .map(|(n, v)| format!("{n}={v}"))
                .collect();
            let inputs = f.kind.inputs();
            let _ = writeln!(
                s,
                "  #{} {} [{}]{} {} :: {}{}",
                f.id,
                f.name,
                f.kind.label(),
                if inputs.is_empty() {
                    String::new()
                } else {
                    format!(
                        " inputs {}",
                        inputs
                            .iter()
                            .map(|i| format!("#{i}"))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                },
                fields.join(" "),
                status,
                if Some(f.id) == self.selected {
                    " :: SELECTED"
                } else {
                    ""
                }
            );
        }
        if let Some(ev) = &self.eval {
            for b in &ev.bodies {
                let _ = writeln!(
                    s,
                    "body #{} {}: volume {} mm3, centroid ({}, {}, {}), bbox ({}, {}, {})..({}, {}, {}), {} tris",
                    b.feature,
                    b.name,
                    fmt_num(b.volume.round()),
                    fmt_num(b.centroid[0]),
                    fmt_num(b.centroid[1]),
                    fmt_num(b.centroid[2]),
                    fmt_num(b.bbox.0[0]),
                    fmt_num(b.bbox.0[1]),
                    fmt_num(b.bbox.0[2]),
                    fmt_num(b.bbox.1[0]),
                    fmt_num(b.bbox.1[1]),
                    fmt_num(b.bbox.1[2]),
                    b.mesh.tri_count
                );
            }
        }
        if self.evaluating() {
            s.push_str("kernel: evaluating (wait, then observe again)\n");
        }
        match self.selected_name() {
            Some(n) => {
                let _ = writeln!(
                    s,
                    "selected: {n} :: its fields are the inputs in the SELECTED panel (type into them by label, e.g. size.x); SUPPRESS / REMOVE buttons; ADD BOX / ADD CYLINDER / MOVE / ROTATE add features; UNION / CUT / INTERSECT use the selection as A then wait for you to click the row of B"
                );
            }
            None => s.push_str("selected: none\n"),
        }
        if let Some(op) = self.pending {
            let _ = writeln!(
                s,
                "pending: {} :: click the feature row to use as B (ESC cancels)",
                op.label()
            );
        }
        if let Some(OpenDialog {
            state,
            closing: false,
        }) = &self.dialog
        {
            match state {
                DialogState::File { op, path, .. } => {
                    let _ = writeln!(
                        s,
                        "dialog: {} :: PATH input = {path:?} (~ and relative paths ok; Tab completes) :: buttons CANCEL / {}",
                        op.title(),
                        op.verb()
                    );
                }
                DialogState::Overwrite { path, .. } => {
                    let _ = writeln!(
                        s,
                        "dialog: {} exists :: buttons CANCEL / OVERWRITE",
                        path.display()
                    );
                }
                DialogState::Error(line) => {
                    let _ = writeln!(s, "dialog: ERROR {line} :: button ACKNOWLEDGE");
                }
            }
        }
        s.push_str("keys: 1-4 views, F fit, cmd+z undo, cmd+s save, cmd+l open (in-app path dialog; cmd+o is the macOS panel you cannot see), cmd+e export stl, cmd+backspace remove\n");
        if self.settings.log_open {
            s.push_str("log (latest last):\n");
        } else {
            s.push_str("log (panel collapsed; latest last):\n");
        }
        let skip = self.log.len().saturating_sub(6);
        for e in &self.log[skip..] {
            let _ = writeln!(s, "  {} {}", e.time, e.text);
        }
        s
    }

    /// Path shown in the title.
    pub(crate) fn title_path(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            None => "unsaved".into(),
        }
    }

    pub(crate) fn file_exists(p: &Path) -> bool {
        p.exists()
    }
}
