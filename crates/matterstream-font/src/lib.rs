//! # matterstream-font
//!
//! Font shaping, MSDF atlas generation, and glyph table management for the
//! mtd1 rendering pipeline.
//!
//! - **`shaper`** — rustybuzz (HarfBuzz port) text shaping
//! - **`atlas`** — Multi-Channel Signed Distance Field (MSDF) atlas builder
//! - **`glyph_table`** — GPU-uploadable glyph metrics table

pub mod shaper;
pub mod atlas;
pub mod glyph_table;

pub use shaper::{TextShaper, ShapedGlyph, ShapedRun};
pub use atlas::{FontAtlas, FontAtlasBuilder};
pub use glyph_table::GlyphEntry;
