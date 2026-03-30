use anyhow::Result;

pub mod fonts;
pub use fonts::FontAtlasBin;

#[cfg(feature = "ttf")]
pub use fonts::{FontAtlas, GlyphInfo};

pub type LoaderResult<T> = Result<T>;
pub type LoaderError = anyhow::Error;

pub struct Loader;

impl Loader {
    pub fn new() -> Self {
        Self {}
    }

    pub fn load_something(&self, path: &str) -> LoaderResult<String> {
        Ok(format!("Loaded content from {}", path))
    }
}
