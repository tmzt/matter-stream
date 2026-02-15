use rusttype::{Font, Scale};
use image::{ImageBuffer, Rgba};
use std::collections::HashMap;

pub struct FontAtlas {
    pub font_data: Vec<u8>,
    pub texture: ImageBuffer<Rgba<u8>, Vec<u8>>,
    pub glyph_map: HashMap<char, GlyphInfo>,
}

pub struct GlyphInfo {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub uv_x: f32,
    pub uv_y: f32,
    pub uv_width: f32,
    pub uv_height: f32,
    pub advance_width: f32,
}

impl FontAtlas {
    pub fn new(font_data: Vec<u8>, font_size: f32) -> Self {
        let _font = Font::try_from_bytes(&font_data).expect("failed to load font");
        let _scale = Scale::uniform(font_size);
        let texture = ImageBuffer::new(256, 256);
        let glyph_map = HashMap::new();

        Self {
            font_data,
            texture,
            glyph_map,
        }
    }

    pub fn get_glyph_info(&self, c: char) -> Option<&GlyphInfo> {
        self.glyph_map.get(&c)
    }
}
