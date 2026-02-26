//! Tier 0 — Global Uniforms (BIOS analog)
//!
//! Shared state: time, theme atoms.

/// Global uniforms accessible to all ops.
#[derive(Debug, Clone)]
pub struct GlobalUniforms {
    time: f32,
    theme_atoms: Vec<f32>,
}

impl GlobalUniforms {
    pub fn new() -> Self {
        Self {
            time: 0.0,
            theme_atoms: Vec::new(),
        }
    }

    pub async fn set_time(&mut self, t: f32) {
        self.time = t;
    }

    pub async fn time(&self) -> f32 {
        self.time
    }

    pub async fn set_theme_atoms(&mut self, atoms: Vec<f32>) {
        self.theme_atoms = atoms;
    }

    pub async fn theme_atoms(&self) -> &[f32] {
        &self.theme_atoms
    }
}

impl Default for GlobalUniforms {
    fn default() -> Self {
        Self::new()
    }
}
