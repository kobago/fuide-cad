//! The CAD's own MCP tools: structured edits of the document (the generic `click` / `type`
//! still work, but `add_feature` / `set_field` / `set_param` say exactly what changed) plus
//! `measure`, `export` / `open` and `view`. Each call goes through [`Action`]s so it is
//! logged and undoable like a click.

use super::*;
use crate::doc::Feature;
use fuide::agent::{ToolCall, ToolSpec};
use serde_json::{json, Value};

pub fn tools() -> Vec<ToolSpec> {
    let obj = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false });
    vec![
        ToolSpec {
            name: "document".into(),
            description: "The whole document as JSON: `params` (name = expression) and `features` in order. Every value is an expression in mm (numbers, + - * / ( ), parameter names, sqrt/sin/cos/min/max). Feature kinds: box {origin[3], size[3]}, cylinder {base[3], axis x|y|z, radius, height}, thread {base[3], axis, diameter, pitch, length} (external ISO thread, union it with a shank / head), gear {center[3], axis, module, teeth, width, pressure} (involute spur gear; centre distance of a pair = module * (z1 + z2) / 2, rotate one by 180/z degrees so the teeth mesh), boolean {op union|cut|intersect, a, b} (consumes both inputs), translate {target, by[3]}, rotate {target, origin[3], axis, angle°}. Bodies not consumed by a later feature are the result.".into(),
            schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "add_feature".into(),
            description: "Append a feature. Pass the feature's JSON fields as in `document` (without `id`), e.g. {\"kind\":\"box\",\"name\":\"PLATE\",\"origin\":[\"0\",\"0\",\"0\"],\"size\":[\"w\",\"d\",\"6\"]} or {\"kind\":\"cut\",\"a\":1,\"b\":2} (`union` / `cut` / `intersect` are shorthand for kind boolean). Returns the new id; the kernel re-evaluates in the background — `wait` then `measure`.".into(),
            schema: json!({ "type": "object", "properties": { "kind": { "type": "string" }, "name": { "type": "string" } }, "required": ["kind"], "additionalProperties": true }),
        },
        ToolSpec {
            name: "set_field".into(),
            description: "Change one field of a feature: an expression field (`origin.x`, `size.z`, `base.y`, `radius`, `height`, `by.x`, `angle`), `axis` (x|y|z), `name`, or `suppressed` (true|false).".into(),
            schema: obj(json!({ "id": { "type": "integer" }, "field": { "type": "string" }, "value": { "type": "string" } }), &["id", "field", "value"]),
        },
        ToolSpec {
            name: "remove_feature".into(),
            description: "Remove a feature (refused while a later feature uses it).".into(),
            schema: obj(json!({ "id": { "type": "integer" } }), &["id"]),
        },
        ToolSpec {
            name: "set_param".into(),
            description: "Add or change a named parameter (`w` = `60`, `hole` = `w / 8`). Parameters may use the ones defined before them.".into(),
            schema: obj(json!({ "name": { "type": "string" }, "value": { "type": "string" } }), &["name", "value"]),
        },
        ToolSpec {
            name: "remove_param".into(),
            description: "Remove a parameter (features using it will error until fixed).".into(),
            schema: obj(json!({ "name": { "type": "string" } }), &["name"]),
        },
        ToolSpec {
            name: "select".into(),
            description: "Select a feature (highlighted in the list and the viewport; `null` clears).".into(),
            schema: obj(json!({ "id": { "type": ["integer", "null"] } }), &[]),
        },
        ToolSpec {
            name: "measure".into(),
            description: "Volume (mm³), bounding box, centroid and triangle count of a result body (`id`) or of every result body. Errors of the last evaluation are included.".into(),
            schema: obj(json!({ "id": { "type": "integer" } }), &[]),
        },
        ToolSpec {
            name: "view".into(),
            description: "Camera and display: preset iso|front|top|right, mode shaded|wire|xray, fit to the bodies. Follow with `screenshot` to look.".into(),
            schema: obj(json!({ "preset": { "type": "string", "enum": ["iso", "front", "top", "right"] }, "mode": { "type": "string", "enum": ["shaded", "wire", "xray"] }, "fit": { "type": "boolean" } }), &[]),
        },
        ToolSpec {
            name: "export".into(),
            description: "Write the result bodies as binary STL (`format` stl, the default) or save the document as JSON (`format` json). `~` and relative paths are fine. Overwriting an existing file is reserved for the human unless the settings allow the agent to confirm.".into(),
            schema: obj(json!({ "path": { "type": "string" }, "format": { "type": "string", "enum": ["stl", "json"] } }), &["path"]),
        },
        ToolSpec {
            name: "open".into(),
            description: "Open a document JSON, replacing the current one (refused while there are unsaved changes unless `discard` is true). `new` = true starts an empty document instead.".into(),
            schema: obj(json!({ "path": { "type": "string" }, "discard": { "type": "boolean" }, "new": { "type": "boolean" } }), &[]),
        },
    ]
}

fn s<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

fn id(args: &Value, k: &str) -> Result<FeatureId, String> {
    args.get(k)
        .and_then(Value::as_u64)
        .map(|v| v as FeatureId)
        .ok_or_else(|| format!("`{k}` (integer feature id) is required"))
}

impl CadApp {
    /// Run one of [`tools`]; the text comes back to the client ahead of the observation, and
    /// the cursor flies to the row / input the tool touched so the person watching sees it.
    pub(crate) fn handle_tool(
        &mut self,
        ctx: &egui::Context,
        call: ToolCall,
        t: f64,
    ) -> Result<(String, Option<String>), String> {
        let text = self.run_tool(ctx, &call, t)?;
        let a = &call.args;
        let row = |app: &Self, id: FeatureId| app.doc.get(id).map(|f| f.name.clone());
        let focus = match call.name.as_str() {
            "add_feature" | "select" | "remove_feature" => {
                self.selected.and_then(|id| row(self, id))
            }
            "set_field" => match s(a, "field") {
                Some("name") | Some("suppressed") | Some("axis") => {
                    id(a, "id").ok().and_then(|i| row(self, i))
                }
                Some(f) => Some(f.to_uppercase()),
                None => None,
            },
            "set_param" => s(a, "name").map(|n| n.trim().to_uppercase()),
            "measure" | "view" => Some("VIEWPORT".into()),
            _ => None,
        };
        Ok((text, focus))
    }

    /// The tool itself; the cursor flies to the row / input it touched afterwards.
    fn run_tool(&mut self, ctx: &egui::Context, call: &ToolCall, t: f64) -> Result<String, String> {
        let a = &call.args;
        match call.name.as_str() {
            "document" => Ok(format!(
                "revision {} :: {}\n{}",
                self.revision,
                match &self.path {
                    Some(p) => p.display().to_string(),
                    None => "unsaved".into(),
                },
                self.doc.to_json()
            )),
            "add_feature" => {
                let mut v = a.clone();
                let obj = v.as_object_mut().ok_or("arguments must be an object")?;
                let kind = obj
                    .get("kind")
                    .and_then(Value::as_str)
                    .ok_or("`kind` is required")?
                    .to_lowercase();
                if let Some(op) = match kind.as_str() {
                    "union" | "cut" | "intersect" => Some(kind.clone()),
                    _ => None,
                } {
                    obj.insert("kind".into(), json!("boolean"));
                    obj.insert("op".into(), json!(op));
                }
                let next = self.doc.next_id();
                obj.insert("id".into(), json!(next));
                if !obj.contains_key("name") {
                    obj.insert("name".into(), json!(""));
                }
                let mut f: Feature =
                    serde_json::from_value(v).map_err(|e| format!("bad feature: {e}"))?;
                if f.name.trim().is_empty() {
                    f.name = format!("{} {}", f.kind.label(), next);
                }
                for i in f.kind.inputs() {
                    if self.doc.get(i).is_none() {
                        return Err(format!("input #{i} does not exist"));
                    }
                }
                let name = f.name.clone();
                let kind = f.kind.clone();
                self.checked(t, |app| {
                    app.apply(ctx, Action::Add(kind), t);
                    app.apply(
                        ctx,
                        Action::Rename {
                            id: next,
                            name: name.clone(),
                        },
                        t,
                    );
                })?;
                Ok(format!(
                    "added #{next} {name} :: evaluating (wait, then measure)"
                ))
            }
            "set_field" => {
                let fid = id(a, "id")?;
                let field = s(a, "field")
                    .ok_or("`field` is required")?
                    .trim()
                    .to_string();
                let value = s(a, "value")
                    .ok_or("`value` (string) is required")?
                    .trim()
                    .to_string();
                let f = self
                    .doc
                    .get(fid)
                    .ok_or_else(|| format!("no feature #{fid}"))?;
                let action = match field.as_str() {
                    "name" => Action::Rename {
                        id: fid,
                        name: value.clone(),
                    },
                    "suppressed" => {
                        let want = matches!(value.as_str(), "true" | "1" | "yes" | "on");
                        if want == f.suppressed {
                            return Ok(format!(
                                "#{fid} already {}",
                                if want { "suppressed" } else { "unsuppressed" }
                            ));
                        }
                        Action::ToggleSuppress(fid)
                    }
                    "axis" => {
                        let mut kind = f.kind.clone();
                        set_axis(&mut kind, &value)?;
                        // apply through the document directly (axis is not an expression field)
                        self.checked(t, |app| {
                            app.snapshot_pub(t);
                            app.doc.get_mut(fid).unwrap().kind = kind;
                            app.touch_pub(t);
                        })?;
                        return Ok(format!("#{fid} axis = {value}"));
                    }
                    _ => {
                        if !f.kind.fields().iter().any(|(n, _)| *n == field) {
                            let names: Vec<String> =
                                f.kind.fields().into_iter().map(|(n, _)| n).collect();
                            return Err(format!(
                                "{} #{fid} has no field `{field}` (fields: {}, name, suppressed)",
                                f.kind.label(),
                                names.join(", ")
                            ));
                        }
                        Action::SetField {
                            id: fid,
                            field: field.clone(),
                            value: value.clone(),
                        }
                    }
                };
                self.checked(t, |app| app.apply(ctx, action, t))?;
                Ok(format!("#{fid} {field} = {value} :: evaluating"))
            }
            "remove_feature" => {
                let fid = id(a, "id")?;
                let name = self
                    .doc
                    .get(fid)
                    .ok_or_else(|| format!("no feature #{fid}"))?
                    .name
                    .clone();
                self.doc.clone().remove(fid)?;
                self.checked(t, |app| app.apply(ctx, Action::Remove(fid), t))?;
                Ok(format!("removed #{fid} {name}"))
            }
            "set_param" => {
                let name = s(a, "name").ok_or("`name` is required")?.trim().to_string();
                let value = s(a, "value")
                    .ok_or("`value` (string) is required")?
                    .trim()
                    .to_string();
                self.doc.clone().set_param(&name, &value)?;
                self.checked(t, |app| {
                    app.apply(
                        ctx,
                        Action::SetParam {
                            name: name.clone(),
                            value: value.clone(),
                        },
                        t,
                    )
                })?;
                let (vals, errs) = eval::resolve_params(&self.doc);
                match vals.get(&name) {
                    Some(v) => Ok(format!("{name} = {value} ({}) :: evaluating", fmt_num(*v))),
                    None => Err(errs
                        .into_iter()
                        .find(|e| e.contains(&format!("parameter {name} ")))
                        .unwrap_or_else(|| "parameter did not resolve".into())),
                }
            }
            "remove_param" => {
                let name = s(a, "name").ok_or("`name` is required")?.trim().to_string();
                if self.doc.param(&name).is_none() {
                    return Err(format!("no parameter `{name}`"));
                }
                self.apply(ctx, Action::RemoveParam(name.clone()), t);
                Ok(format!("removed parameter {name}"))
            }
            "select" => {
                let fid = a.get("id").and_then(Value::as_u64).map(|v| v as FeatureId);
                if let Some(i) = fid {
                    if self.doc.get(i).is_none() {
                        return Err(format!("no feature #{i}"));
                    }
                }
                self.pending = None;
                self.apply(ctx, Action::Select(fid), t);
                Ok(match self.selected_name() {
                    Some(n) => format!("selected {n}"),
                    None => "selection cleared".into(),
                })
            }
            "measure" => {
                let Some(ev) = &self.eval else {
                    return Err("nothing evaluated yet".into());
                };
                let mut out = String::new();
                if self.evaluating() {
                    out.push_str("note: the kernel is still evaluating the latest change; these numbers are from the previous evaluation\n");
                }
                let want = a.get("id").and_then(Value::as_u64).map(|v| v as FeatureId);
                let bodies: Vec<&crate::eval::Body> = ev
                    .bodies
                    .iter()
                    .filter(|b| want.is_none_or(|w| w == b.feature))
                    .collect();
                if bodies.is_empty() {
                    let empty = want.is_some_and(|w| {
                        ev.notes
                            .iter()
                            .any(|n| n.contains(&format!("(#{w}): the result is empty")))
                    });
                    let _ = writeln!(
                        out,
                        "{}",
                        match (want, empty) {
                            (Some(w), true) => format!("#{w}: the result is empty (volume 0)"),
                            (Some(w), false) => format!(
                                "no result body for #{w} (is it consumed by a later feature?)"
                            ),
                            (None, _) => "no result body".into(),
                        }
                    );
                }
                for b in bodies {
                    let sz = [
                        b.bbox.1[0] - b.bbox.0[0],
                        b.bbox.1[1] - b.bbox.0[1],
                        b.bbox.1[2] - b.bbox.0[2],
                    ];
                    let f = |v: f64| fmt_num((v * 1000.0).round() / 1000.0);
                    let _ = writeln!(
                        out,
                        "#{} {}: volume {} mm3 ({} cm3) :: size {} x {} x {} mm :: bbox min ({}, {}, {}) max ({}, {}, {}) :: centroid ({}, {}, {}) :: {} triangles, {} edges",
                        b.feature, b.name, f(b.volume), f(b.volume / 1000.0), f(sz[0]), f(sz[1]), f(sz[2]),
                        f(b.bbox.0[0]), f(b.bbox.0[1]), f(b.bbox.0[2]), f(b.bbox.1[0]), f(b.bbox.1[1]), f(b.bbox.1[2]),
                        f(b.centroid[0]), f(b.centroid[1]), f(b.centroid[2]), b.mesh.tri_count, b.mesh.edges.len()
                    );
                }
                for e in &ev.errors {
                    let who = match self.doc.get(e.feature) {
                        Some(f) => format!("#{} {}", f.id, f.name),
                        None => "document".into(),
                    };
                    let _ = writeln!(out, "error {who}: {}", e.message);
                }
                Ok(out.trim_end().to_string())
            }
            "view" => {
                let mut done = Vec::new();
                if let Some(p) = s(a, "preset") {
                    let preset = match p.to_lowercase().as_str() {
                        "iso" => Preset::Iso,
                        "front" => Preset::Front,
                        "top" => Preset::Top,
                        "right" => Preset::Right,
                        other => return Err(format!("unknown preset `{other}`")),
                    };
                    self.apply(ctx, Action::Preset(preset), t);
                    done.push(format!("preset {p}"));
                }
                if let Some(m) = s(a, "mode") {
                    let mode = match m.to_lowercase().as_str() {
                        "shaded" => ViewMode::Shaded,
                        "wire" => ViewMode::Wire,
                        "xray" | "x-ray" => ViewMode::Xray,
                        other => return Err(format!("unknown mode `{other}`")),
                    };
                    self.apply(ctx, Action::Mode(mode), t);
                    done.push(format!("mode {m}"));
                }
                if a.get("fit").and_then(Value::as_bool).unwrap_or(false) {
                    self.apply(ctx, Action::Fit, t);
                    done.push("fit".into());
                }
                if done.is_empty() {
                    return Err("nothing to do: pass preset, mode and/or fit".into());
                }
                Ok(format!("view :: {}", done.join(", ")))
            }
            "export" => {
                let path = fuide::pathinput::expand(
                    s(a, "path").ok_or("`path` is required")?,
                    &Self::cwd(),
                );
                let op = match s(a, "format").unwrap_or("stl").to_lowercase().as_str() {
                    "stl" => FileOp::ExportStl,
                    "json" => FileOp::Save,
                    other => return Err(format!("unknown format `{other}`")),
                };
                if path.exists() && !self.settings.agent_confirm {
                    return Err(format!(
                        "{} exists; overwriting is reserved for the human (ask them, or Settings → AGENT → CONFIRM DIALOGS → AGENT)",
                        path.display()
                    ));
                }
                if op == FileOp::ExportStl && self.eval.as_ref().is_none_or(|e| e.bodies.is_empty())
                {
                    return Err("no result bodies to export".into());
                }
                let shown = path.display().to_string();
                self.checked(t, |app| app.run_file_op_pub(op, path, t))?;
                Ok(format!("{} :: {shown}", op.verb().to_lowercase()))
            }
            "open" => {
                if a.get("new").and_then(Value::as_bool).unwrap_or(false) {
                    if self.dirty && !a.get("discard").and_then(Value::as_bool).unwrap_or(false) {
                        return Err(
                            "unsaved changes; pass discard: true or export the document first"
                                .into(),
                        );
                    }
                    self.apply(ctx, Action::New, t);
                    return Ok("new document".into());
                }
                let path = fuide::pathinput::expand(
                    s(a, "path").ok_or("`path` (or new: true) is required")?,
                    &Self::cwd(),
                );
                if self.dirty && !a.get("discard").and_then(Value::as_bool).unwrap_or(false) {
                    return Err(
                        "unsaved changes; pass discard: true or export the document first".into(),
                    );
                }
                let shown = path.display().to_string();
                self.checked(t, |app| app.run_file_op_pub(FileOp::Open, path, t))?;
                Ok(format!(
                    "opened {shown} :: {} features, {} params :: evaluating",
                    self.doc.features.len(),
                    self.doc.params.len()
                ))
            }
            other => Err(format!("unknown tool `{other}`")),
        }
    }

    /// Run `f`; if it queued an error card, swallow the card and return its log line instead
    /// (the tool caller gets the detail, the human is not interrupted).
    fn checked(&mut self, _t: f64, f: impl FnOnce(&mut Self)) -> Result<(), String> {
        let errors = self.error_queue.len();
        let logs = self.log.len();
        f(self);
        if self.error_queue.len() > errors {
            self.error_queue.truncate(errors);
            let detail = self.log[logs..]
                .iter()
                .rev()
                .find(|e| e.level == Level::Danger)
                .map(|e| e.text.clone())
                .unwrap_or_else(|| "failed".into());
            return Err(detail);
        }
        Ok(())
    }
}

pub(crate) fn set_axis_pub(kind: &mut FeatureKind, value: &str) -> Result<(), String> {
    set_axis(kind, value)
}

fn set_axis(kind: &mut FeatureKind, value: &str) -> Result<(), String> {
    let axis = match value.to_lowercase().as_str() {
        "x" => Axis::X,
        "y" => Axis::Y,
        "z" => Axis::Z,
        other => return Err(format!("axis must be x, y or z, not `{other}`")),
    };
    match kind {
        FeatureKind::Cylinder { axis: a, .. }
        | FeatureKind::Thread { axis: a, .. }
        | FeatureKind::Gear { axis: a, .. }
        | FeatureKind::Rotate { axis: a, .. } => {
            *a = axis;
            Ok(())
        }
        other => Err(format!("{} has no axis", other.label())),
    }
}
