//! fuide-3d — the 3D viewport shared by FUIDE apps (CAD, simulator).
//!
//! - [`math`]: `Vec3` / column-major `Mat4` (no external linear algebra crate)
//! - [`camera`]: Z-up orbit camera (yaw / pitch / distance), presets, fit-to-bounds, projection
//! - [`scene`]: CPU-side meshes and line batches, a version counter for GPU re-upload
//! - [`renderer`]: wgpu pipelines — hologram fill (fresnel + scan lines) and glowing screen-space
//!   lines — rendered off-screen (MSAA 4x, own depth) into a texture egui paints
//! - [`viewport`]: the egui widget: input (orbit / pan / zoom), the render call, the HUD triad
//!
//! The renderer draws premultiplied alpha over a transparent clear, so the panel's ground shows
//! through: the 3D pass is a layer inside the FUI, not a separate world.

pub mod camera;
pub mod math;
pub mod renderer;
pub mod scene;
pub mod viewport;

pub use camera::{OrbitCamera, Preset};
pub use math::{Mat4, Vec3};
pub use renderer::{Renderer, Style};
pub use scene::{LineBatch, MeshData, Scene};
pub use viewport::{ViewMode, Viewport};
