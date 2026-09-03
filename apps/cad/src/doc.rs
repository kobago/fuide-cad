//! The document: named parameters and an ordered list of features. Each feature makes a body
//! (primitives) or consumes bodies made by earlier features (booleans, transforms). Bodies not
//! consumed by a later feature are the result. This is the whole model both the GUI and the MCP
//! tools edit, and it round-trips through JSON.

use serde::{Deserialize, Serialize};

pub type FeatureId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }
    pub fn unit(self) -> [f64; 3] {
        match self {
            Axis::X => [1.0, 0.0, 0.0],
            Axis::Y => [0.0, 1.0, 0.0],
            Axis::Z => [0.0, 0.0, 1.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoolOp {
    Union,
    Cut,
    Intersect,
}

impl BoolOp {
    pub const ALL: [BoolOp; 3] = [BoolOp::Union, BoolOp::Cut, BoolOp::Intersect];
    pub fn label(self) -> &'static str {
        match self {
            BoolOp::Union => "UNION",
            BoolOp::Cut => "CUT",
            BoolOp::Intersect => "INTERSECT",
        }
    }
}

/// Values are expressions (see [`crate::expr`]) so `width / 2` works everywhere.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FeatureKind {
    /// Axis-aligned box from `origin` (minimum corner) of `size`.
    Box {
        origin: [String; 3],
        size: [String; 3],
    },
    /// Cylinder from `base` along `axis`.
    Cylinder {
        base: [String; 3],
        axis: Axis,
        radius: String,
        height: String,
    },
    /// External ISO-style thread from `base` along `axis` (major `diameter`, `pitch`,
    /// `length`), as a mesh. Union it with a head or a shank core.
    Thread {
        base: [String; 3],
        axis: Axis,
        diameter: String,
        pitch: String,
        length: String,
    },
    /// Involute spur gear standing on `center` along `axis`: `module`, `teeth`, face
    /// `width`, `pressure` angle (degrees). Cut a bore with a cylinder.
    Gear {
        center: [String; 3],
        axis: Axis,
        module: String,
        teeth: String,
        width: String,
        pressure: String,
    },
    /// `a op b`; consumes both.
    Boolean {
        op: BoolOp,
        a: FeatureId,
        b: FeatureId,
    },
    /// Moves `target`; consumes it.
    Translate { target: FeatureId, by: [String; 3] },
    /// Rotates `target` about the axis through `origin`; consumes it. Degrees.
    Rotate {
        target: FeatureId,
        origin: [String; 3],
        axis: Axis,
        angle: String,
    },
}

impl FeatureKind {
    pub fn label(&self) -> &'static str {
        match self {
            FeatureKind::Box { .. } => "BOX",
            FeatureKind::Cylinder { .. } => "CYLINDER",
            FeatureKind::Thread { .. } => "THREAD",
            FeatureKind::Gear { .. } => "GEAR",
            FeatureKind::Boolean { op, .. } => op.label(),
            FeatureKind::Translate { .. } => "TRANSLATE",
            FeatureKind::Rotate { .. } => "ROTATE",
        }
    }
    /// Feature ids this one consumes.
    pub fn inputs(&self) -> Vec<FeatureId> {
        match self {
            FeatureKind::Box { .. }
            | FeatureKind::Cylinder { .. }
            | FeatureKind::Thread { .. }
            | FeatureKind::Gear { .. } => vec![],
            FeatureKind::Boolean { a, b, .. } => vec![*a, *b],
            FeatureKind::Translate { target, .. } | FeatureKind::Rotate { target, .. } => {
                vec![*target]
            }
        }
    }
    /// `(label, expression)` pairs for the parameter panel and the MCP `set_param` tool.
    pub fn fields(&self) -> Vec<(String, String)> {
        let xyz = |prefix: &str, v: &[String; 3]| {
            ["x", "y", "z"]
                .iter()
                .zip(v.iter())
                .map(|(s, e)| (format!("{prefix}.{s}"), e.clone()))
                .collect::<Vec<_>>()
        };
        match self {
            FeatureKind::Box { origin, size } => {
                let mut f = xyz("origin", origin);
                f.extend(xyz("size", size));
                f
            }
            FeatureKind::Cylinder {
                base,
                radius,
                height,
                ..
            } => {
                let mut f = xyz("base", base);
                f.push(("radius".into(), radius.clone()));
                f.push(("height".into(), height.clone()));
                f
            }
            FeatureKind::Thread {
                base,
                diameter,
                pitch,
                length,
                ..
            } => {
                let mut f = xyz("base", base);
                f.push(("diameter".into(), diameter.clone()));
                f.push(("pitch".into(), pitch.clone()));
                f.push(("length".into(), length.clone()));
                f
            }
            FeatureKind::Gear {
                center,
                module,
                teeth,
                width,
                pressure,
                ..
            } => {
                let mut f = xyz("center", center);
                f.push(("module".into(), module.clone()));
                f.push(("teeth".into(), teeth.clone()));
                f.push(("width".into(), width.clone()));
                f.push(("pressure".into(), pressure.clone()));
                f
            }
            FeatureKind::Boolean { .. } => vec![],
            FeatureKind::Translate { by, .. } => xyz("by", by),
            FeatureKind::Rotate { origin, angle, .. } => {
                let mut f = xyz("origin", origin);
                f.push(("angle".into(), angle.clone()));
                f
            }
        }
    }
    /// Set the expression of a field named as in [`fields`](Self::fields).
    pub fn set_field(&mut self, name: &str, value: &str) -> Result<(), String> {
        fn xyz<'a>(v: &'a mut [String; 3], prefix: &str, name: &str) -> Option<&'a mut String> {
            let rest = name.strip_prefix(prefix)?.strip_prefix('.')?;
            let i = match rest {
                "x" => 0,
                "y" => 1,
                "z" => 2,
                _ => return None,
            };
            Some(&mut v[i])
        }
        let slot = match self {
            FeatureKind::Box { origin, size } => {
                xyz(origin, "origin", name).or_else(|| xyz(size, "size", name))
            }
            FeatureKind::Cylinder {
                base,
                radius,
                height,
                ..
            } => match name {
                "radius" => Some(radius),
                "height" => Some(height),
                _ => xyz(base, "base", name),
            },
            FeatureKind::Thread {
                base,
                diameter,
                pitch,
                length,
                ..
            } => match name {
                "diameter" => Some(diameter),
                "pitch" => Some(pitch),
                "length" => Some(length),
                _ => xyz(base, "base", name),
            },
            FeatureKind::Gear {
                center,
                module,
                teeth,
                width,
                pressure,
                ..
            } => match name {
                "module" => Some(module),
                "teeth" => Some(teeth),
                "width" => Some(width),
                "pressure" => Some(pressure),
                _ => xyz(center, "center", name),
            },
            FeatureKind::Boolean { .. } => None,
            FeatureKind::Translate { by, .. } => xyz(by, "by", name),
            FeatureKind::Rotate { origin, angle, .. } => match name {
                "angle" => Some(angle),
                _ => xyz(origin, "origin", name),
            },
        };
        match slot {
            Some(s) => {
                *s = value.trim().to_string();
                Ok(())
            }
            None => Err(format!("{} has no field '{name}'", self.label())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub name: String,
    #[serde(flatten)]
    pub kind: FeatureKind,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub suppressed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    #[serde(default)]
    pub params: Vec<Param>,
    #[serde(default)]
    pub features: Vec<Feature>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new("untitled")
    }
}

impl Document {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
            features: Vec::new(),
        }
    }

    pub fn next_id(&self) -> FeatureId {
        self.features.iter().map(|f| f.id + 1).max().unwrap_or(1)
    }

    /// Append a feature; the name defaults to `KIND n`.
    pub fn add(&mut self, kind: FeatureKind, name: Option<&str>) -> FeatureId {
        let id = self.next_id();
        let name = name
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} {id}", kind.label()));
        self.features.push(Feature {
            id,
            name,
            kind,
            suppressed: false,
        });
        id
    }

    pub fn get(&self, id: FeatureId) -> Option<&Feature> {
        self.features.iter().find(|f| f.id == id)
    }

    pub fn get_mut(&mut self, id: FeatureId) -> Option<&mut Feature> {
        self.features.iter_mut().find(|f| f.id == id)
    }

    pub fn index_of(&self, id: FeatureId) -> Option<usize> {
        self.features.iter().position(|f| f.id == id)
    }

    /// Remove a feature. Fails if a later feature consumes it.
    pub fn remove(&mut self, id: FeatureId) -> Result<Feature, String> {
        let idx = self
            .index_of(id)
            .ok_or_else(|| format!("no feature #{id}"))?;
        if let Some(user) = self.features.iter().find(|f| f.kind.inputs().contains(&id)) {
            return Err(format!(
                "#{id} is used by {} (#{}); remove that first",
                user.name, user.id
            ));
        }
        Ok(self.features.remove(idx))
    }

    /// Features (not suppressed) whose body is not consumed by a later feature.
    pub fn is_result(&self, id: FeatureId) -> bool {
        self.get(id).is_some_and(|f| !f.suppressed)
            && !self
                .features
                .iter()
                .any(|f| !f.suppressed && f.kind.inputs().contains(&id))
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    /// Add or replace a parameter.
    pub fn set_param(&mut self, name: &str, value: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Err(format!("'{name}' is not a valid parameter name"));
        }
        match self.params.iter_mut().find(|p| p.name == name) {
            Some(p) => p.value = value.trim().to_string(),
            None => self.params.push(Param {
                name: name.to_string(),
                value: value.trim().to_string(),
            }),
        }
        Ok(())
    }

    pub fn remove_param(&mut self, name: &str) -> bool {
        let before = self.params.len();
        self.params.retain(|p| p.name != name);
        self.params.len() != before
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("document serialises")
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let doc: Document = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut seen = std::collections::HashSet::new();
        for f in &doc.features {
            if !seen.insert(f.id) {
                return Err(format!("duplicate feature id #{}", f.id));
            }
            for i in f.kind.inputs() {
                if !seen.contains(&i) {
                    return Err(format!(
                        "{} (#{}) uses #{i} which is not an earlier feature",
                        f.name, f.id
                    ));
                }
            }
        }
        Ok(doc)
    }
}

/// Helpers for building features from numbers (tests, defaults).
pub fn xyz(x: f64, y: f64, z: f64) -> [String; 3] {
    [x, y, z].map(fmt_num)
}

pub fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holed_plate() -> Document {
        let mut d = Document::new("plate");
        d.set_param("w", "40").unwrap();
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
                radius: "5".into(),
                height: "20".into(),
            },
            None,
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
    fn ids_names_and_results() {
        let d = holed_plate();
        assert_eq!(
            d.features.iter().map(|f| f.id).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(d.features[1].name, "CYLINDER 2");
        assert_eq!(d.features[2].name, "CUT 3");
        assert!(!d.is_result(1) && !d.is_result(2) && d.is_result(3));
    }

    #[test]
    fn json_round_trip_and_validation() {
        let d = holed_plate();
        let text = d.to_json();
        assert!(text.contains("\"kind\": \"boolean\"") && text.contains("\"op\": \"cut\""));
        assert!(!text.contains("suppressed"));
        assert_eq!(Document::from_json(&text).unwrap(), d);
        let bad = text.replace("\"a\": 1", "\"a\": 9");
        assert!(Document::from_json(&bad).unwrap_err().contains("uses #9"));
    }

    #[test]
    fn remove_refuses_used_features() {
        let mut d = holed_plate();
        assert!(d.remove(1).unwrap_err().contains("used by CUT 3"));
        d.remove(3).unwrap();
        d.remove(1).unwrap();
        assert_eq!(d.features.len(), 1);
        assert_eq!(d.next_id(), 3);
    }

    #[test]
    fn fields_get_and_set() {
        let mut d = holed_plate();
        let f = d.get_mut(2).unwrap();
        assert_eq!(f.kind.fields()[3], ("radius".to_string(), "5".to_string()));
        f.kind.set_field("radius", " 6 ").unwrap();
        f.kind.set_field("base.z", "-10").unwrap();
        assert_eq!(f.kind.fields()[2].1, "-10");
        assert!(f
            .kind
            .set_field("depth", "1")
            .unwrap_err()
            .contains("no field"));
    }

    #[test]
    fn params_validate_names() {
        let mut d = Document::new("p");
        assert!(d.set_param("2x", "1").is_err());
        assert!(d.set_param("", "1").is_err());
        d.set_param("len", "10").unwrap();
        d.set_param("len", "12").unwrap();
        assert_eq!(d.params.len(), 1);
        assert_eq!(d.param("len").unwrap().value, "12");
        assert!(d.remove_param("len") && !d.remove_param("len"));
    }

    #[test]
    fn number_formatting() {
        assert_eq!(fmt_num(40.0), "40");
        assert_eq!(fmt_num(-2.5), "-2.5");
        assert_eq!(fmt_num(0.1 + 0.2), "0.3");
    }
}
