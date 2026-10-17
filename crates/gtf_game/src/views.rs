//! Reference views (`content/freeway/views.toml`, PLAN.md §7.2): named still cameras for look
//! checks and performance measurement, loaded as `freeway/view/<name>`.

use anyhow::{Result, anyhow};
use glam::Vec3;
use pav_core::present::CameraIntent;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    #[serde(default)]
    pub about: String,
    pub eye: Vec3,
    pub target: Vec3,
    #[serde(default = "fov")]
    pub fov: f32,
    /// "HH:MM" (time of day) and a weather name, applied when the view loads.
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub weather: Option<String>,
}

fn fov() -> f32 {
    55.0
}

#[derive(Deserialize)]
struct File {
    view: Vec<View>,
}

/// Every view in the file.
pub fn all() -> Result<Vec<View>> {
    let text = crate::content::text("views.toml", include_str!("../../../content/freeway/views.toml"));
    let f: File = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/views.toml: {e}"))?;
    Ok(f.view)
}

/// The view called `name`.
pub fn find(name: &str) -> Result<View> {
    let views = all()?;
    let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
    views.iter().find(|v| v.name == name).cloned().ok_or_else(|| anyhow!("no view '{name}' (views: {})", names.join(", ")))
}

impl View {
    pub fn camera(&self) -> CameraIntent {
        CameraIntent {
            eye: self.eye,
            target: self.target,
            up: Vec3::Y,
            fov: self.fov,
            near: 0.3,
            far: 2500.0,
            cut: false,
            shake: 0.0,
        }
    }
}
