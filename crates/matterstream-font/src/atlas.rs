//! MSDF atlas builder — generates Multi-Channel Signed Distance Field textures
//! from font glyphs via `msdfgen`.
//!
//! The atlas packs glyphs into a single texture using shelf-based bin packing.
//! Each glyph is rendered as a 3-channel (RGB) MSDF bitmap.

use crate::glyph_table::GlyphEntry;

/// MSDF atlas containing packed glyph bitmaps and their metrics.
#[derive(Debug, Clone)]
pub struct FontAtlas {
    /// Atlas texture width in pixels
    pub width: u32,
    /// Atlas texture height in pixels
    pub height: u32,
    /// Number of channels (3 = MSDF, 4 = MTSDF)
    pub channels: u32,
    /// Raw pixel data: width * height * channels bytes, row-major
    pub pixel_data: Vec<u8>,
    /// Per-glyph entries indexed by position in the table
    pub glyphs: Vec<GlyphEntry>,
    /// Map from glyph_id to index in `glyphs`
    glyph_index: std::collections::HashMap<u16, usize>,
    /// Fraction from cell top to baseline (e.g. 0.75 = baseline at 75% from top).
    /// Used by the shader to align glyphs on the baseline.
    pub baseline_frac: f32,
}

impl FontAtlas {
    /// Look up a glyph entry by glyph ID.
    pub fn get_glyph(&self, glyph_id: u16) -> Option<&GlyphEntry> {
        self.glyph_index.get(&glyph_id).map(|&idx| &self.glyphs[idx])
    }

    /// Serialize the atlas to bytes for embedding in mtd1.
    ///
    /// Format:
    /// ```text
    /// [4b width][4b height][4b num_glyphs][4b channels]  -- 16 byte header
    /// [GlyphEntry * num_glyphs]                          -- 24 bytes each
    /// [pixel_data]                                       -- width*height*channels bytes
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_size = 16;
        let entries_size = self.glyphs.len() * GlyphEntry::PACKED_SIZE;
        let pixel_size = self.pixel_data.len();
        let total = header_size + entries_size + pixel_size;

        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&self.width.to_le_bytes());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&(self.glyphs.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.channels.to_le_bytes());

        for entry in &self.glyphs {
            buf.extend_from_slice(&entry.to_bytes());
        }

        buf.extend_from_slice(&self.pixel_data);
        buf
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 16 {
            return Err("atlas data too short");
        }

        let width = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let height = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let num_glyphs = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let channels = u32::from_le_bytes(data[12..16].try_into().unwrap());

        let entries_start = 16;
        let entries_end = entries_start + num_glyphs * GlyphEntry::PACKED_SIZE;
        if data.len() < entries_end {
            return Err("atlas data too short for glyph entries");
        }

        let mut glyphs = Vec::with_capacity(num_glyphs);
        let mut glyph_index = std::collections::HashMap::new();
        for i in 0..num_glyphs {
            let offset = entries_start + i * GlyphEntry::PACKED_SIZE;
            let entry = GlyphEntry::from_bytes(&data[offset..offset + GlyphEntry::PACKED_SIZE])?;
            glyph_index.insert(entry.glyph_id, i);
            glyphs.push(entry);
        }

        let pixel_start = entries_end;
        let expected_pixels = (width * height * channels) as usize;
        if data.len() < pixel_start + expected_pixels {
            return Err("atlas data too short for pixel data");
        }

        let pixel_data = data[pixel_start..pixel_start + expected_pixels].to_vec();

        Ok(Self {
            width,
            height,
            channels,
            pixel_data,
            glyphs,
            baseline_frac: 0.75, // default for deserialized atlases
            glyph_index,
        })
    }
}

/// Builder for constructing MSDF font atlases.
pub struct FontAtlasBuilder {
    font_data: Vec<u8>,
    glyph_size: u32,
    px_range: f64,
    /// Queued glyph IDs to generate
    queued_glyphs: Vec<u16>,
}

/// Shelf-based bin packer for atlas layout.
struct ShelfPacker {
    width: u32,
    shelves: Vec<Shelf>,
}

struct Shelf {
    y: u32,
    height: u32,
    x_cursor: u32,
}

impl ShelfPacker {
    fn new(width: u32) -> Self {
        Self {
            width,
            shelves: Vec::new(),
        }
    }

    /// Pack a rectangle of (w, h). Returns (x, y) position or None if full.
    fn pack(&mut self, w: u32, h: u32) -> (u32, u32) {
        // Try existing shelves
        for shelf in &mut self.shelves {
            if shelf.height >= h && shelf.x_cursor + w <= self.width {
                let x = shelf.x_cursor;
                let y = shelf.y;
                shelf.x_cursor += w;
                return (x, y);
            }
        }

        // New shelf
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        self.shelves.push(Shelf {
            y,
            height: h,
            x_cursor: w,
        });
        (0, y)
    }

    /// Total height used so far.
    fn used_height(&self) -> u32 {
        self.shelves.last().map_or(0, |s| s.y + s.height)
    }
}

impl FontAtlasBuilder {
    /// Create a builder for a given font.
    ///
    /// `glyph_size` is the MSDF bitmap size per glyph (e.g., 32 or 48).
    /// `px_range` is the distance field range in pixels (typically 4.0-8.0).
    pub fn new(font_data: Vec<u8>, glyph_size: u32, px_range: f64) -> Self {
        Self {
            font_data,
            glyph_size,
            px_range,
            queued_glyphs: Vec::new(),
        }
    }

    /// Queue a single glyph ID for atlas generation.
    pub fn add_glyph(&mut self, glyph_id: u16) {
        if !self.queued_glyphs.contains(&glyph_id) {
            self.queued_glyphs.push(glyph_id);
        }
    }

    /// Queue all glyphs for a codepoint range (resolves via cmap).
    pub fn add_codepoint_range(&mut self, start: char, end: char) {
        let face = match ttf_parser::Face::parse(&self.font_data, 0) {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut gids = Vec::new();
        for cp in (start as u32)..=(end as u32) {
            if let Some(ch) = char::from_u32(cp) {
                if let Some(gid) = face.glyph_index(ch) {
                    gids.push(gid.0);
                }
            }
        }
        for gid in gids {
            self.add_glyph(gid);
        }
    }

    /// Queue common Latin + digit + punctuation glyphs.
    pub fn add_ascii(&mut self) {
        self.add_codepoint_range(' ', '~');
    }

    /// Build the MSDF atlas from all queued glyphs.
    pub fn build(&self) -> Result<FontAtlas, String> {
        use msdfgen::{Bitmap, FontExt, Framing, MsdfGeneratorConfig, Range, Rgb, FillRule};

        // Use ttf-parser 0.25 for metrics
        let face25 = ttf_parser::Face::parse(&self.font_data, 0)
            .map_err(|e| format!("font parse error: {e}"))?;
        // Use ttf-parser 0.18 for msdfgen (FontExt trait)
        let face18 = ttf_parser_018::Face::parse(&self.font_data, 0)
            .map_err(|e| format!("font parse error (v18): {e}"))?;

        let gs = self.glyph_size;
        let padded = gs + 2; // 1px padding on each side

        // Calculate atlas dimensions
        let num_glyphs = self.queued_glyphs.len();
        let cols = ((num_glyphs as f64).sqrt().ceil() as u32).max(1);
        let atlas_w = cols * padded;

        let channels = 3u32; // MSDF = RGB
        let mut packer = ShelfPacker::new(atlas_w);

        // First pass: generate MSDF for each glyph and record positions
        struct GlyphResult {
            glyph_id: u16,
            atlas_x: u32,
            atlas_y: u32,
            advance_x: f32,
            bearing_x: f32,
            bearing_y: f32,
            msdf_data: Vec<u8>,
        }

        let mut results = Vec::with_capacity(num_glyphs);

        for &glyph_id in &self.queued_glyphs {
            // Metrics from v0.25 — all normalized to [0,1] relative to em square
            let upem = face25.units_per_em() as f32;
            let gid25 = ttf_parser::GlyphId(glyph_id);
            let advance_x = face25.glyph_hor_advance(gid25).unwrap_or(0) as f32 / upem;
            let bbox = face25.glyph_bounding_box(gid25);
            let (bearing_x, bearing_y) = bbox
                .map(|b| (b.x_min as f32 / upem, b.y_max as f32 / upem))
                .unwrap_or((0.0, 0.0));

            // Pack into atlas
            let (atlas_x, atlas_y) = packer.pack(padded, padded);

            // Skip non-printing glyphs (space, control chars) — no outlines to render.
            // They get a zero-filled atlas cell; the shader uses advance-only for spacing.
            let gid18 = ttf_parser_018::GlyphId(glyph_id);
            let has_outline = face18.glyph_shape(gid18).is_some()
                && face25.glyph_bounding_box(gid25).is_some();

            let mut msdf_data = vec![0u8; (gs * gs * channels) as usize];

            if has_outline {
                let mut bitmap: Bitmap<Rgb<f32>> = Bitmap::new(gs, gs);
                if let Some(mut shape) = face18.glyph_shape(gid18) {
                    // Use a uniform projection based on the em square so all glyphs
                    // share the same scale. This preserves relative sizes (lowercase
                    // letters are smaller than uppercase in the atlas cell).
                    //
                    // Projection maps font units → atlas pixels:
                    //   scale = atlas_size / em_size (with padding margin)
                    //   translate = offset to center in cell
                    let margin = self.px_range;
                    let usable = gs as f64 - 2.0 * margin;

                    // Use the font's global bounding box (head table yMin/yMax)
                    // for the actual glyph extremes — covers all descenders and accents.
                    let global_bbox = face25.global_bounding_box();
                    let y_min = global_bbox.y_min as f64; // deepest descender (negative)
                    let y_max = global_bbox.y_max as f64; // tallest ascender (positive)
                    let total_height = y_max - y_min;

                    // Scale to fit total vertical extent in usable cell area
                    let em_scale = usable / total_height;

                    // X: left margin
                    let tx = margin;
                    // Y: font origin is at baseline (y=0 in font coords).
                    // Shift so y_min maps to bottom of cell (margin from edge).
                    let ty = margin + (-y_min) * em_scale;

                    let framing = Framing {
                        range: self.px_range,
                        projection: msdfgen::Projection::new(
                            msdfgen::Vector2::new(em_scale, em_scale),
                            msdfgen::Vector2::new(tx, ty),
                        ),
                    };

                    shape.edge_coloring_simple(3.0, 0);
                    shape.generate_msdf(&mut bitmap, &framing, MsdfGeneratorConfig::default());
                    shape.correct_sign(&mut bitmap, &framing, FillRule::default());
                }

                let inv_range = 0.5 / self.px_range as f32;
                for y in 0..gs {
                    for x in 0..gs {
                        let pixel = bitmap.pixel(x, y);
                        let idx = ((y * gs + x) * channels) as usize;
                        msdf_data[idx] = msdf_to_u8(pixel.r, inv_range);
                        msdf_data[idx + 1] = msdf_to_u8(pixel.g, inv_range);
                        msdf_data[idx + 2] = msdf_to_u8(pixel.b, inv_range);
                    }
                }
            }

            results.push(GlyphResult {
                glyph_id,
                atlas_x: atlas_x + 1,
                atlas_y: atlas_y + 1,
                advance_x,
                bearing_x,
                bearing_y,
                msdf_data,
            });
        }

        // Finalize atlas dimensions
        let atlas_h = packer.used_height().max(1);
        let mut pixel_data = vec![0u8; (atlas_w * atlas_h * channels) as usize];
        let mut glyphs = Vec::with_capacity(num_glyphs);
        let mut glyph_index = std::collections::HashMap::new();

        for result in &results {
            // Blit MSDF into atlas
            for row in 0..gs {
                for col in 0..gs {
                    let src_idx = ((row * gs + col) * channels) as usize;
                    let dst_idx =
                        (((result.atlas_y + row) * atlas_w + result.atlas_x + col) * channels)
                            as usize;
                    if src_idx + 2 < result.msdf_data.len()
                        && dst_idx + 2 < pixel_data.len()
                    {
                        pixel_data[dst_idx] = result.msdf_data[src_idx];
                        pixel_data[dst_idx + 1] = result.msdf_data[src_idx + 1];
                        pixel_data[dst_idx + 2] = result.msdf_data[src_idx + 2];
                    }
                }
            }

            let entry = GlyphEntry {
                glyph_id: result.glyph_id,
                atlas_x: result.atlas_x as u16,
                atlas_y: result.atlas_y as u16,
                atlas_w: gs as u16,
                atlas_h: gs as u16,
                advance_x: result.advance_x,
                bearing_x: result.bearing_x,
                bearing_y: result.bearing_y,
            };
            glyph_index.insert(result.glyph_id, glyphs.len());
            glyphs.push(entry);
        }

        // Compute baseline fraction from global bbox.
        // baseline is at ty from the bottom of the cell.
        // In screen coordinates (Y down), baseline_frac = 1 - ty / gs.
        let global_bbox = face25.global_bounding_box();
        let y_min = global_bbox.y_min as f64;
        let y_max = global_bbox.y_max as f64;
        let total_height = y_max - y_min;
        let margin = self.px_range;
        let usable = gs as f64 - 2.0 * margin;
        let em_scale = usable / total_height;
        let ty = margin + (-y_min) * em_scale;
        let baseline_frac = (1.0 - ty / gs as f64) as f32;

        Ok(FontAtlas {
            width: atlas_w,
            height: atlas_h,
            channels,
            pixel_data,
            glyphs,
            glyph_index,
            baseline_frac,
        })
    }
}

/// Map a raw MSDF signed distance to u8.
/// `inv_range` = 0.5 / px_range.
/// Map raw MSDF signed distance to u8.
/// Map raw MSDF distance to u8 [0,255].
/// Map raw MSDF distance to u8 [0,255].
/// msdfgen with TrueType: positive = outside, negative = inside.
/// No sign flip — store raw normalized distance. Shader handles convention.
fn msdf_to_u8(value: f32, inv_range: f32) -> u8 {
    let normalized = (value * inv_range + 0.5).clamp(0.0, 1.0);
    (normalized * 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_system_font() -> Option<Vec<u8>> {
        let paths = [
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/SFNS.ttf",
            "/Library/Fonts/Arial.ttf",
        ];
        for path in &paths {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
        None
    }

    #[test]
    fn build_ascii_atlas() {
        let font_data = match load_system_font() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: no system font");
                return;
            }
        };

        let mut builder = FontAtlasBuilder::new(font_data, 32, 4.0);
        builder.add_ascii();
        let atlas = builder.build().expect("atlas build failed");

        assert!(atlas.width > 0);
        assert!(atlas.height > 0);
        assert_eq!(atlas.channels, 3);
        assert!(!atlas.glyphs.is_empty());
        assert!(!atlas.pixel_data.is_empty());

        println!(
            "Atlas: {}x{}, {} glyphs, {} bytes pixel data",
            atlas.width,
            atlas.height,
            atlas.glyphs.len(),
            atlas.pixel_data.len()
        );

        // Verify some glyphs have non-zero MSDF data
        let has_content = atlas.pixel_data.iter().any(|&b| b != 0);
        assert!(has_content, "atlas should have non-zero pixel data");
    }

    #[test]
    fn atlas_roundtrip() {
        let font_data = match load_system_font() {
            Some(d) => d,
            None => {
                eprintln!("SKIP: no system font");
                return;
            }
        };

        let mut builder = FontAtlasBuilder::new(font_data, 32, 4.0);
        builder.add_codepoint_range('A', 'Z');
        let atlas = builder.build().expect("build failed");
        let bytes = atlas.to_bytes();
        let parsed = FontAtlas::from_bytes(&bytes).expect("parse failed");

        assert_eq!(atlas.width, parsed.width);
        assert_eq!(atlas.height, parsed.height);
        assert_eq!(atlas.glyphs.len(), parsed.glyphs.len());
        assert_eq!(atlas.pixel_data.len(), parsed.pixel_data.len());
    }
}
