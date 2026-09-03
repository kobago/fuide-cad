//! Frame layout and panels: feature list (left top), document parameters (left bottom),
//! toolbar + viewport (centre), selected feature + measurements (right), event log (bottom),
//! and the file / overwrite / error dialogs.

use super::*;
use egui::{pos2, vec2, Align2, Rect, RichText, Sense};
use fuide::table::{self, Cell, Column, Width};
use fuide::widgets::{self, LogLine};
use fuide::{mono, palette, type_scale, Dialog, Panel, Shell};

impl eframe::App for CadApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        if let Some(rs) = frame.wgpu_render_state() {
            self.viewport.attach(rs);
        }
        self.devshot.tick(ui.ctx());
        self.agent.set_enabled(ui.ctx(), self.settings.agent);
        self.agent.set_blocked(self.agent_blocked());
        let agent_state = self.agent.wants_state().then(|| self.agent_state());
        self.agent.tick(ui.ctx(), agent_state);
        let t = ui.input(|i| i.time);
        let ctx = ui.ctx().clone();
        if let Some(call) = self.agent.take_tool() {
            let name = call.name.clone();
            let result = self.handle_tool(&ctx, call, t);
            match &result {
                Ok((text, _)) => self.push_log(
                    t,
                    format!("agent // {name} :: {}", text.lines().next().unwrap_or("")),
                    Level::Info,
                ),
                Err(e) => {
                    self.push_log(t, format!("agent // {name} :: refused :: {e}"), Level::Warn)
                }
            }
            let (result, focus) = match result {
                Ok((text, focus)) => (Ok(text), focus),
                Err(e) => (Err(e), None),
            };
            self.agent.finish_tool(result, focus.as_deref());
        }
        self.poll(t);
        self.sync_scene();
        let pal = palette(ui.ctx());
        let fps = 1.0 / ui.input(|i| i.stable_dt).max(1e-3);

        let mut actions: Vec<Action> = Vec::new();
        if self.dialog.is_none() {
            if let Some(line) = self.error_queue.pop_front() {
                self.dialog = Some(OpenDialog {
                    state: DialogState::Error(line),
                    closing: false,
                });
            }
        }
        self.handle_keys(ui, &mut actions);

        let (bodies, tris) = self
            .eval
            .as_ref()
            .map(|e| (e.bodies.len(), e.tri_count()))
            .unwrap_or((0, 0));
        let errors = self.eval.as_ref().map(|e| e.errors.len()).unwrap_or(0);
        let mut shell = Shell::new(APP_NAME)
            .subtitle(format!(
                "v0.1 :: {} :: {}{}",
                self.doc.name.to_uppercase(),
                self.title_path().to_uppercase(),
                if self.dirty { " *" } else { "" }
            ))
            .status_left(format!(
                "{} :: {} FEATURES :: {} BODIES :: {} TRIS :: {}x{} :: {:.0} FPS",
                fuide::fmt::uptime(t),
                self.doc.features.len(),
                bodies,
                tris,
                self.viewport.last_size[0],
                self.viewport.last_size[1],
                fps
            ))
            .lamp(
                "KERNEL",
                if self.evaluating() { pal.warn } else { pal.ok },
                self.evaluating(),
            )
            .settings_button(true);
        if errors > 0 {
            shell = shell.lamp(format!("{errors} ERR"), pal.danger, false);
        }
        if let Some(op) = self.pending {
            shell = shell.lamp(format!("{} :: PICK B", op.label()), pal.warn, true);
        }
        if let Some((text, busy)) = self.agent.lamp() {
            shell = shell.lamp(text, if busy { pal.warn } else { pal.accent }, busy);
        }

        let log_open = self.settings.log_open;
        let mut log_resized = false;
        let mut log_toggled = false;
        let k_log = ctx.animate_bool_with_time_and_easing(
            egui::Id::new("cad-log-open"),
            log_open,
            0.24,
            egui::emath::easing::cubic_out,
        );
        let out = shell.show_full(ui, |ui| {
            let c = ui.max_rect();
            let log_max = c.height() - BODY_MIN;
            self.log_h = self.log_h.clamp(LOG_MIN, log_max.max(LOG_MIN));
            let log_h = egui::lerp(LOG_CLOSED..=self.log_h, k_log);
            let log_rect = Rect::from_min_max(pos2(c.left(), c.bottom() - log_h), c.max);
            let body_top = c.top() + 10.0; // room for the title chips
            let body_bottom = log_rect.top() - GAP - 8.0;
            let left = Rect::from_min_max(
                pos2(c.left(), body_top),
                pos2(c.left() + LEFT_W, body_bottom),
            );
            let right = Rect::from_min_max(
                pos2(c.right() - RIGHT_W, body_top),
                pos2(c.right(), body_bottom),
            );
            let center = Rect::from_min_max(
                pos2(left.right() + GAP, body_top),
                pos2(right.left() - GAP, body_bottom),
            );
            let params = Rect::from_min_max(pos2(left.left(), left.bottom() - PARAMS_H), left.max);
            let features =
                Rect::from_min_max(left.min, pos2(left.right(), params.top() - GAP - 8.0));
            let toolbar =
                Rect::from_min_size(center.min, vec2(center.width(), TOOLBAR_H * TOOLBAR_ROWS));
            let view = Rect::from_min_max(pos2(center.left(), toolbar.bottom() + GAP), center.max);
            let measure_h = 150.0;
            let selected = Rect::from_min_max(
                right.min,
                pos2(right.right(), right.bottom() - measure_h - GAP - 8.0),
            );
            let measure =
                Rect::from_min_max(pos2(right.left(), right.bottom() - measure_h), right.max);

            self.ui_features(ui, features, &mut actions);
            self.ui_params(ui, params, &mut actions);
            self.ui_toolbar(ui, toolbar, &mut actions);
            self.ui_viewport(ui, view, t, &mut actions);
            self.ui_selected(ui, selected, &mut actions);
            self.ui_measure(ui, measure);
            if log_open && k_log >= 1.0 {
                let strip = Rect::from_min_max(
                    pos2(c.left(), body_bottom),
                    pos2(c.right(), log_rect.top()),
                );
                let resp = widgets::h_splitter(
                    ui,
                    strip,
                    "log",
                    &mut self.log_h,
                    LOG_MIN,
                    log_max,
                    "LOG HEIGHT",
                );
                log_resized = resp.drag_stopped();
            }
            log_toggled = self.ui_log(ui, log_rect, log_open, k_log > 0.0);
        });
        self.agent.paint(&ctx);
        if out.settings_clicked {
            actions.push(Action::OpenSettings);
        }
        if log_resized {
            self.settings.log_height = Some(self.log_h.round());
            self.save_settings(t);
        }
        if log_toggled {
            self.settings.log_open = !log_open;
            self.save_settings(t);
        }

        self.ui_dialog(&ctx, &mut actions);
        for a in actions {
            self.apply(&ctx, a, t);
        }
        if self.evaluating() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.settings_win
            .set_agent_status(&ctx, &self.agent.status_line());
        if self.settings_win.show(&ctx, &mut self.settings, APP_NAME) {
            self.settings_changed(t);
            self.scene_for = (0, None);
        }
    }
}

fn feature_columns() -> [Column; 4] {
    [
        Column::new("NAME", Width::Flex).unsortable(),
        Column::new("KIND", Width::Chars(9.0)).unsortable(),
        Column::new("STATE", Width::Chars(7.0)).unsortable(),
        Column::new("#", Width::Chars(3.0)).right().unsortable(),
    ]
}

impl CadApp {
    fn ui_features(&mut self, ui: &mut Ui, rect: Rect, actions: &mut Vec<Action>) {
        let pal = palette(ui.ctx());
        let ts = type_scale(ui.ctx());
        let columns = feature_columns();
        let doc = &self.doc;
        let eval = &self.eval;
        let pending = self.pending;
        let mut state = std::mem::take(&mut self.table);
        let tag = match pending {
            Some(op) => format!("{} :: pick B", op.label().to_lowercase()),
            None => format!("{} features", doc.features.len()),
        };
        let resp = Panel::new("Features")
            .tag(
                tag,
                if pending.is_some() {
                    pal.warn
                } else {
                    pal.text_dim
                },
            )
            .padding(8.0, 14.0)
            .show_rect(ui, rect, |ui| {
                if doc.features.is_empty() {
                    let r = ui.max_rect();
                    theme::display_text(
                        ui.painter(),
                        r.center(),
                        Align2::CENTER_CENTER,
                        "NO FEATURES",
                        ts.title,
                        pal.accent_dim,
                    );
                    ui.painter().text(
                        r.center() + vec2(0.0, ts.title + 6.0),
                        Align2::CENTER_CENTER,
                        "ADD BOX / ADD CYLINDER TO START",
                        mono(ts.label),
                        pal.text_dim,
                    );
                    return table::TableResponse::default();
                }
                table::table(
                    ui,
                    "features",
                    &columns,
                    doc.features.len(),
                    &mut state,
                    |row, col| {
                        let f = &doc.features[row];
                        match col {
                            0 => {
                                if f.suppressed {
                                    Cell::dim(f.name.clone())
                                } else {
                                    Cell::text(f.name.clone())
                                }
                            }
                            1 => Cell::tag(f.kind.label()),
                            3 => Cell::dim(format!("{}", f.id)),
                            _ => {
                                if f.suppressed {
                                    Cell::dim("OFF")
                                } else if eval.as_ref().is_some_and(|e| e.error_for(f.id).is_some())
                                {
                                    Cell::tag("ERROR").color(pal.danger)
                                } else if doc.is_result(f.id) {
                                    Cell::tag("BODY").color(pal.ok)
                                } else {
                                    Cell::dim("USED")
                                }
                            }
                        }
                    },
                )
            });
        self.table = state;
        if let Some(r) = resp.clicked.or(resp.secondary_clicked) {
            actions.push(Action::Select(self.doc.features.get(r).map(|f| f.id)));
        }
        if let Some(r) = resp.double_clicked {
            if let Some(f) = self.doc.features.get(r) {
                actions.push(Action::ToggleSuppress(f.id));
            }
        }
    }

    fn ui_params(&mut self, ui: &mut Ui, rect: Rect, actions: &mut Vec<Action>) {
        let pal = palette(ui.ctx());
        let ts = type_scale(ui.ctx());
        let (vals, _) = eval::resolve_params(&self.doc);
        let params = self.doc.params.clone();
        Panel::new("Parameters")
            .tag(format!("{}", params.len()), pal.text_dim)
            .padding(8.0, 12.0)
            .show_rect(ui, rect, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let name_w = 74.0;
                egui::ScrollArea::vertical()
                    .id_salt("params")
                    .max_height(ui.available_height() - ts.row - 10.0)
                    .show(ui, |ui| {
                        for p in &params {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let (nr, _) =
                                    ui.allocate_exact_size(vec2(name_w, ts.row), Sense::hover());
                                ui.painter().text(
                                    pos2(nr.right(), nr.center().y),
                                    Align2::RIGHT_CENTER,
                                    &p.name,
                                    mono(ts.data),
                                    pal.accent,
                                );
                                let mut value = p.value.clone();
                                let resp = widgets::text_input(ui, 90.0, &mut value, &p.name);
                                if resp.changed() {
                                    actions.push(Action::SetParam {
                                        name: p.name.clone(),
                                        value,
                                    });
                                }
                                let shown = match vals.get(&p.name) {
                                    Some(v) => fmt_num((*v * 1000.0).round() / 1000.0),
                                    None => "ERR".into(),
                                };
                                let (vr, _) =
                                    ui.allocate_exact_size(vec2(56.0, ts.row), Sense::hover());
                                ui.painter().text(
                                    pos2(vr.left() + 2.0, vr.center().y),
                                    Align2::LEFT_CENTER,
                                    shown,
                                    mono(ts.label),
                                    if vals.contains_key(&p.name) {
                                        pal.text_dim
                                    } else {
                                        pal.danger
                                    },
                                );
                                if widgets::icon_button(
                                    ui,
                                    vec2(ts.row, ts.row),
                                    widgets::Icon::Cross,
                                    true,
                                )
                                .on_hover_text("remove parameter")
                                .clicked()
                                {
                                    actions.push(Action::RemoveParam(p.name.clone()));
                                }
                            });
                        }
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let name = widgets::text_input(ui, name_w, &mut self.new_param.0, "name");
                    let value = widgets::text_input(ui, 90.0, &mut self.new_param.1, "value");
                    let ok =
                        !self.new_param.0.trim().is_empty() && !self.new_param.1.trim().is_empty();
                    let submit = ok
                        && (name.lost_focus() || value.lost_focus())
                        && ui.input(|i| i.key_pressed(Key::Enter));
                    if widgets::button(ui, vec2(52.0, ts.row), "ADD", ok).clicked() || submit {
                        actions.push(Action::SetParam {
                            name: self.new_param.0.clone(),
                            value: self.new_param.1.clone(),
                        });
                    }
                });
            });
    }

    fn ui_toolbar(&mut self, ui: &mut Ui, rect: Rect, actions: &mut Vec<Action>) {
        let pal = palette(ui.ctx());
        let ts = type_scale(ui.ctx());
        let row1 = Rect::from_min_size(rect.min, vec2(rect.width(), TOOLBAR_H));
        let row2 = Rect::from_min_size(
            pos2(rect.left(), rect.top() + TOOLBAR_H),
            vec2(rect.width(), TOOLBAR_H),
        );
        let has_sel = self.selected.is_some();
        let pending = self.pending.is_some();

        // row 1: modelling verbs
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row1)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui1 = &mut child;
        ui1.spacing_mut().item_spacing.x = 6.0;
        let (lr, _) = ui1.allocate_exact_size(vec2(30.0, ts.row), Sense::hover());
        ui1.painter().text(
            pos2(lr.left(), lr.center().y),
            Align2::LEFT_CENTER,
            "ADD",
            mono(ts.label),
            pal.text_dim,
        );
        if widgets::button(ui1, vec2(48.0, ts.row), "BOX", !pending).clicked() {
            actions.push(Action::Add(FeatureKind::Box {
                origin: xyz(0.0, 0.0, 0.0),
                size: xyz(40.0, 30.0, 20.0),
            }));
        }
        if widgets::button(ui1, vec2(78.0, ts.row), "CYLINDER", !pending).clicked() {
            actions.push(Action::Add(FeatureKind::Cylinder {
                base: xyz(0.0, 0.0, 0.0),
                axis: Axis::Z,
                radius: "10".into(),
                height: "30".into(),
            }));
        }
        if widgets::button(ui1, vec2(62.0, ts.row), "THREAD", !pending).clicked() {
            actions.push(Action::Add(FeatureKind::Thread {
                base: xyz(0.0, 0.0, 0.0),
                axis: Axis::Z,
                diameter: "8".into(),
                pitch: "1.25".into(),
                length: "20".into(),
            }));
        }
        if widgets::button(ui1, vec2(52.0, ts.row), "GEAR", !pending).clicked() {
            actions.push(Action::Add(FeatureKind::Gear {
                center: xyz(0.0, 0.0, 0.0),
                axis: Axis::Z,
                module: "1".into(),
                teeth: "20".into(),
                width: "6".into(),
                pressure: "20".into(),
            }));
        }
        ui1.add_space(8.0);
        for op in BoolOp::ALL {
            let on = self.pending == Some(op);
            let color = if on { pal.warn } else { pal.accent };
            let w = match op {
                BoolOp::Union => 58.0,
                BoolOp::Cut => 46.0,
                BoolOp::Intersect => 82.0,
            };
            if widgets::button_colored(ui1, vec2(w, ts.row), op.label(), has_sel, color).clicked() {
                actions.push(if on {
                    Action::CancelPending
                } else {
                    Action::BeginBoolean(op)
                });
            }
        }
        ui1.add_space(8.0);
        if widgets::button(ui1, vec2(50.0, ts.row), "MOVE", has_sel && !pending).clicked() {
            if let Some(id) = self.selected {
                actions.push(Action::Add(FeatureKind::Translate {
                    target: id,
                    by: xyz(0.0, 0.0, 0.0),
                }));
            }
        }
        if widgets::button(ui1, vec2(60.0, ts.row), "ROTATE", has_sel && !pending).clicked() {
            if let Some(id) = self.selected {
                actions.push(Action::Add(FeatureKind::Rotate {
                    target: id,
                    origin: xyz(0.0, 0.0, 0.0),
                    axis: Axis::Z,
                    angle: "90".into(),
                }));
            }
        }
        if let Some(op) = self.pending {
            let (pr, _) =
                ui1.allocate_exact_size(vec2(ui1.available_width(), ts.row), Sense::hover());
            ui1.painter().text(
                pos2(pr.left() + 4.0, pr.center().y),
                Align2::LEFT_CENTER,
                format!("{} :: CLICK THE ROW OF B // ESC CANCELS", op.label()),
                mono(ts.label),
                pal.warn,
            );
        }

        // row 2: view controls
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row2)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui2 = &mut child;
        ui2.spacing_mut().item_spacing.x = 6.0;
        let (lr, _) = ui2.allocate_exact_size(vec2(30.0, ts.row), Sense::hover());
        ui2.painter().text(
            pos2(lr.left(), lr.center().y),
            Align2::LEFT_CENTER,
            "VIEW",
            mono(ts.label),
            pal.text_dim,
        );
        for p in [Preset::Iso, Preset::Front, Preset::Top, Preset::Right] {
            if widgets::button(ui2, vec2(52.0, ts.row), p.label(), true).clicked() {
                actions.push(Action::Preset(p));
            }
        }
        if widgets::button(ui2, vec2(40.0, ts.row), "FIT", true).clicked() {
            actions.push(Action::Fit);
        }
        ui2.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for m in ViewMode::ALL.iter().rev() {
                let mut on = self.viewport.mode == *m;
                if widgets::toggle_chip(ui, m.label(), &mut on).clicked()
                    && self.viewport.mode != *m
                {
                    actions.push(Action::Mode(*m));
                }
            }
        });
    }

    fn ui_viewport(&mut self, ui: &mut Ui, rect: Rect, t: f64, actions: &mut Vec<Action>) {
        let pal = palette(ui.ctx());
        let ts = type_scale(ui.ctx());
        let tag = format!(
            "{} :: {}",
            self.viewport.mode.label().to_lowercase(),
            match self.viewport.has_gpu() {
                true => "gpu",
                false => "no gpu",
            }
        );
        let evaluating = self.evaluating();
        let empty = self.eval.as_ref().is_none_or(|e| e.bodies.is_empty());
        let mut fit = false;
        Panel::new("Viewport")
            .tag(tag, pal.text_dim)
            .padding(2.0, 10.0)
            .show_rect(ui, rect, |ui| {
                let r = ui.max_rect();
                let resp = self.viewport.show(ui, r, &self.scene, &pal, t);
                fuide::agent::describe(&resp, || {
                    egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "VIEWPORT")
                });
                if resp.double_clicked() {
                    fit = true;
                }
                let p = ui.painter().with_clip_rect(r);
                if empty && !evaluating {
                    theme::display_text(
                        &p,
                        r.center(),
                        Align2::CENTER_CENTER,
                        "NO BODIES",
                        ts.title,
                        pal.accent_dim,
                    );
                }
                let cam = &self.viewport.camera;
                p.text(
                    pos2(r.right() - 8.0, r.top() + 8.0),
                    Align2::RIGHT_TOP,
                    format!(
                        "YAW {:>4.0} :: PITCH {:>3.0} :: DIST {:>7.1}",
                        cam.yaw.to_degrees().rem_euclid(360.0),
                        cam.pitch.to_degrees(),
                        cam.dist
                    ),
                    mono(ts.label),
                    pal.text_dim,
                );
                p.text(
                    pos2(r.right() - 8.0, r.bottom() - 8.0),
                    Align2::RIGHT_BOTTOM,
                    "DRAG ORBIT :: SHIFT+DRAG PAN :: WHEEL ZOOM :: DBL-CLICK FIT",
                    mono(ts.label),
                    pal.text_dim,
                );
                if evaluating {
                    fuide::fx::scan_band(&p, r, t, pal.accent);
                }
            });
        if fit {
            actions.push(Action::Fit);
        }
    }

    fn ui_selected(&mut self, ui: &mut Ui, rect: Rect, actions: &mut Vec<Action>) {
        let pal = palette(ui.ctx());
        let ts = type_scale(ui.ctx());
        let Some(f) = self.selected.and_then(|id| self.doc.get(id)).cloned() else {
            Panel::new("Selected")
                .tag("none", pal.text_dim)
                .padding(8.0, 14.0)
                .show_rect(ui, rect, |ui| {
                    let r = ui.max_rect();
                    theme::display_text(
                        ui.painter(),
                        r.center(),
                        Align2::CENTER_CENTER,
                        "NO SELECTION",
                        ts.title,
                        pal.accent_dim,
                    );
                });
            return;
        };
        let error = self
            .eval
            .as_ref()
            .and_then(|e| e.error_for(f.id))
            .map(str::to_string);
        let is_result = self.doc.is_result(f.id);
        let mut name = match &self.name_edit {
            Some((id, n)) if *id == f.id => n.clone(),
            _ => f.name.clone(),
        };
        let mut name_edit = None;
        let doc = &self.doc;
        Panel::new("Selected")
            .tag(
                format!("#{} {}", f.id, f.kind.label().to_lowercase()),
                pal.text_dim,
            )
            .padding(8.0, 12.0)
            .show_rect(ui, rect, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let resp = widgets::text_input(ui, ui.available_width(), &mut name, "name");
                if resp.changed() {
                    name_edit = Some(name.clone());
                }
                if resp.lost_focus() {
                    actions.push(Action::Rename {
                        id: f.id,
                        name: name.clone(),
                    });
                    name_edit = Some(String::new());
                }
                let inputs = f.kind.inputs();
                if !inputs.is_empty() {
                    let names: Vec<String> = inputs
                        .iter()
                        .map(|i| match doc.get(*i) {
                            Some(g) => format!("#{i} {}", g.name),
                            None => format!("#{i} ?"),
                        })
                        .collect();
                    widgets::readout(ui, "inputs", &names.join(" // "), Some(pal.accent));
                }
                ui.add_space(2.0);
                let label_w = 70.0;
                for (field, value) in f.kind.fields() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (lr, _) = ui.allocate_exact_size(vec2(label_w, ts.row), Sense::hover());
                        ui.painter().text(
                            pos2(lr.right(), lr.center().y),
                            Align2::RIGHT_CENTER,
                            field.to_uppercase(),
                            mono(ts.label),
                            pal.text_dim,
                        );
                        let mut v = value.clone();
                        let resp = widgets::text_input(ui, ui.available_width(), &mut v, &field);
                        if resp.changed() {
                            actions.push(Action::SetField {
                                id: f.id,
                                field: field.clone(),
                                value: v,
                            });
                        }
                    });
                }
                if let FeatureKind::Cylinder { axis, .. } | FeatureKind::Rotate { axis, .. } =
                    &f.kind
                {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let (lr, _) = ui.allocate_exact_size(vec2(label_w, ts.row), Sense::hover());
                        ui.painter().text(
                            pos2(lr.right(), lr.center().y),
                            Align2::RIGHT_CENTER,
                            "AXIS",
                            mono(ts.label),
                            pal.text_dim,
                        );
                        for a in Axis::ALL {
                            let mut on = *axis == a;
                            if widgets::toggle_chip(ui, a.label(), &mut on).clicked() && *axis != a
                            {
                                actions.push(Action::SetField {
                                    id: f.id,
                                    field: "axis".into(),
                                    value: a.label().to_lowercase(),
                                });
                            }
                        }
                    });
                }
                ui.add_space(4.0);
                widgets::rule(ui);
                let state = if f.suppressed {
                    ("SUPPRESSED", pal.text_dim)
                } else if error.is_some() {
                    ("ERROR", pal.danger)
                } else if is_result {
                    ("RESULT BODY", pal.ok)
                } else {
                    ("CONSUMED BY A LATER FEATURE", pal.text_dim)
                };
                widgets::readout(ui, "state", state.0, Some(state.1));
                if let Some(e) = &error {
                    ui.add(
                        egui::Label::new(RichText::new(e).font(mono(ts.label)).color(pal.danger))
                            .wrap(),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let label = if f.suppressed {
                        "UNSUPPRESS"
                    } else {
                        "SUPPRESS"
                    };
                    if widgets::button(ui, vec2(100.0, ts.row), label, true).clicked() {
                        actions.push(Action::ToggleSuppress(f.id));
                    }
                    if widgets::button_colored(ui, vec2(72.0, ts.row), "REMOVE", true, pal.danger)
                        .clicked()
                    {
                        actions.push(Action::Remove(f.id));
                    }
                });
            });
        match name_edit {
            Some(n) if n.is_empty() => self.name_edit = None,
            Some(n) => self.name_edit = Some((f.id, n)),
            None => {}
        }
    }

    fn ui_measure(&self, ui: &mut Ui, rect: Rect) {
        let pal = palette(ui.ctx());
        let body = self.eval.as_ref().and_then(|e| {
            self.selected
                .and_then(|id| e.body_for(id))
                .or(e.bodies.first())
        });
        let tag = match body {
            Some(b) => format!("#{} {}", b.feature, b.name.to_lowercase()),
            None => "no body".into(),
        };
        Panel::new("Measure")
            .tag(tag, pal.text_dim)
            .padding(8.0, 12.0)
            .show_rect(ui, rect, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let Some(b) = body else {
                    widgets::readout(ui, "volume", "--", None);
                    widgets::readout(ui, "size", "--", None);
                    widgets::readout(ui, "centroid", "--", None);
                    return;
                };
                let size = [
                    b.bbox.1[0] - b.bbox.0[0],
                    b.bbox.1[1] - b.bbox.0[1],
                    b.bbox.1[2] - b.bbox.0[2],
                ];
                let f = |v: f64| fmt_num((v * 100.0).round() / 100.0);
                widgets::readout(
                    ui,
                    "volume",
                    &format!("{} CM3", f(b.volume / 1000.0)),
                    Some(pal.accent),
                );
                widgets::readout(
                    ui,
                    "size",
                    &format!("{} x {} x {} MM", f(size[0]), f(size[1]), f(size[2])),
                    None,
                );
                widgets::readout(
                    ui,
                    "min",
                    &format!(
                        "{} / {} / {}",
                        f(b.bbox.0[0]),
                        f(b.bbox.0[1]),
                        f(b.bbox.0[2])
                    ),
                    None,
                );
                widgets::readout(
                    ui,
                    "centroid",
                    &format!(
                        "{} / {} / {}",
                        f(b.centroid[0]),
                        f(b.centroid[1]),
                        f(b.centroid[2])
                    ),
                    None,
                );
                widgets::readout(ui, "triangles", &format!("{}", b.mesh.tri_count), None);
                widgets::readout(ui, "edges", &format!("{}", b.mesh.edges.len()), None);
            });
    }

    fn ui_log(&self, ui: &mut Ui, rect: Rect, open: bool, feed: bool) -> bool {
        let pal = palette(ui.ctx());
        let (_, toggled) = Panel::new("Event log")
            .tag(format!("{} events", self.log.len()), pal.text_dim)
            .padding(8.0, 12.0)
            .show_collapsible_rect(ui, rect, open, |ui| {
                if !feed {
                    return;
                }
                let lines: Vec<LogLine> = self
                    .log
                    .iter()
                    .map(|l| LogLine {
                        time: l.time.clone(),
                        text: l.text.clone(),
                        color: match l.level {
                            Level::Info => pal.text,
                            Level::Ok => pal.ok,
                            Level::Warn => pal.warn,
                            Level::Danger => pal.danger,
                        },
                    })
                    .collect();
                widgets::log_feed(
                    ui,
                    &lines,
                    pal.text_dim,
                    type_scale(ui.ctx()).label,
                    widgets::LogOrder::NewestFirst,
                );
            });
        toggled
    }

    fn ui_dialog(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let pal = palette(ctx);
        let ts = type_scale(ctx);
        let Some(OpenDialog { state, closing }) = &mut self.dialog else {
            return;
        };
        let open = !*closing;
        let enter = open && ctx.input(|i| i.key_pressed(Key::Enter));
        let finished;
        match state {
            DialogState::File {
                op,
                path,
                completions,
            } => {
                let op = *op;
                let title = op.title();
                let tab = open && ctx.input(|i| i.key_pressed(Key::Tab));
                let cwd = CadApp::cwd();
                let resp = Dialog::new(title)
                    .tag(
                        if op == FileOp::ExportStl {
                            "binary stl"
                        } else {
                            "json"
                        },
                        pal.text_dim,
                    )
                    .width(560.0)
                    .show(ctx, open, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        let resp = widgets::text_input(ui, ui.available_width(), path, "PATH");
                        if open {
                            resp.request_focus();
                        }
                        if tab {
                            let list =
                                fuide::pathinput::complete(path, &cwd, op == FileOp::Open, 12);
                            if list.len() == 1 {
                                *path = list[0].clone();
                                completions.clear();
                            } else if !list.is_empty() {
                                *path = fuide::pathinput::common_prefix(&list);
                                *completions = list;
                            }
                            // keep the caret at the end after we replaced the text
                            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
                                let end = egui::text::CCursor::new(path.chars().count());
                                st.cursor
                                    .set_char_range(Some(egui::text::CCursorRange::one(end)));
                                st.store(ui.ctx(), resp.id);
                            }
                        }
                        if !completions.is_empty() {
                            let text = completions
                                .iter()
                                .map(|c| {
                                    std::path::Path::new(c)
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| c.clone())
                                })
                                .collect::<Vec<_>>()
                                .join("  ");
                            ui.add(
                                egui::Label::new(
                                    RichText::new(text).font(mono(ts.label)).color(pal.text_dim),
                                )
                                .wrap(),
                            );
                        }
                        let exists = CadApp::file_exists(&fuide::pathinput::expand(path, &cwd));
                        let note = match (op, exists) {
                            (FileOp::Open, true) => ("FILE FOUND :: ENTER OPENS", pal.ok),
                            (FileOp::Open, false) => {
                                ("~ AND RELATIVE PATHS OK :: TAB COMPLETES", pal.text_dim)
                            }
                            (_, true) => {
                                ("EXISTS :: YOU WILL BE ASKED BEFORE OVERWRITING", pal.warn)
                            }
                            (_, false) => ("NEW FILE :: TAB COMPLETES DIRECTORIES", pal.text_dim),
                        };
                        let (nr, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), ts.row),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            pos2(nr.left() + 2.0, nr.center().y),
                            Align2::LEFT_CENTER,
                            note.0,
                            mono(ts.label),
                            note.1,
                        );
                        ui.add_space(8.0);
                        fuide::dialog::button_row(
                            ui,
                            &[
                                ("CANCEL", pal.text_dim, true),
                                (op.verb(), pal.accent, !path.trim().is_empty()),
                            ],
                        )
                    });
                finished = resp.finished;
                let clicked = resp.inner.flatten();
                if !open {
                } else if resp.should_close || clicked == Some(0) {
                    actions.push(Action::CloseDialog);
                } else if enter || clicked == Some(1) {
                    actions.push(Action::FileGo);
                }
            }
            DialogState::Overwrite { op, path } => {
                let resp = Dialog::new("File exists")
                    .tag(op.verb().to_lowercase(), pal.text_dim)
                    .outline(pal.warn)
                    .width(520.0)
                    .show(ctx, open, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.add(
                            egui::Label::new(
                                RichText::new(path.display().to_string())
                                    .font(mono(ts.data))
                                    .color(pal.accent),
                            )
                            .wrap(),
                        );
                        ui.add_space(2.0);
                        let (nr, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), ts.row),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            pos2(nr.left() + 2.0, nr.center().y),
                            Align2::LEFT_CENTER,
                            "THE EXISTING FILE IS REPLACED",
                            mono(ts.label),
                            pal.warn,
                        );
                        ui.add_space(8.0);
                        fuide::dialog::button_row(
                            ui,
                            &[
                                ("CANCEL", pal.text_dim, true),
                                ("OVERWRITE", pal.warn, true),
                            ],
                        )
                    });
                finished = resp.finished;
                let clicked = resp.inner.flatten();
                if !open {
                } else if resp.should_close || clicked == Some(0) {
                    actions.push(Action::CloseDialog);
                } else if enter || clicked == Some(1) {
                    actions.push(Action::ConfirmDialog);
                }
            }
            DialogState::Error(line) => {
                let resp = fuide::dialog::alert(
                    ctx,
                    open,
                    "ERROR",
                    line,
                    "DETAILS IN THE EVENT LOG",
                    pal.danger,
                );
                finished = resp.finished;
                if open && (resp.should_close || resp.inner == Some(true)) {
                    actions.push(Action::CloseDialog);
                }
            }
        }
        if finished {
            self.dialog = None;
        }
    }
}
