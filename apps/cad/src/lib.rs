//! FUIDE CAD — parametric 3D CAD with a Sci-Fi FUI look, on the truck kernel.
//!
//! - `kernel`: truck behind plain functions (solids, booleans, tessellation, STL); GUI-free
//! - `expr`: the parameter expression language (`width / 2 + 3`)
//! - `doc`: the document — parameters and the ordered feature list; JSON on disk
//! - `eval`: runs a document through the kernel into bodies (meshes, volumes, errors)
//! - `app`: the egui application (feature list, viewport, selected feature, log, MCP agent)

pub mod app;
pub mod doc;
pub mod eval;
pub mod expr;
pub mod filepicker;
pub mod kernel;
pub mod mesh;
