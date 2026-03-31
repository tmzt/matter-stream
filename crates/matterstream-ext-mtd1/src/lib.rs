//! MTD1 OR page handler — document/text layout engine.
//!
//! Processes structured text layout commands (headings, text blocks,
//! tags, cards, grids) and emits SdfDrawCmd entries for GPU rendering.
//! Performs CPU-side word-wrapping and text layout.
//!
//! Can be used directly as a Rust API or via VM OR page dispatch.

use std::any::Any;
use matterstream_common::SdfDrawCmd;
use matterstream_vm::or_page::OrPageHandler;
use matterstream_vm::rpn::RpnError;
use matterstream_vm::vm_handle::VmHandle;
use matterstream_vm_arena::TripleArena;

/// FourCC for MTD1 OR page: `b"MTD1"` = 0x4D544431
pub const FOURCC_MTD1: u32 = 0x4D544431;

/// Text size presets.
const SIZE_H1: f32 = 14.0;
const SIZE_H2: f32 = 12.0;
const SIZE_H3: f32 = 10.0;
const SIZE_BODY: f32 = 8.0;
const SIZE_LABEL: f32 = 8.0;
const SIZE_TAG: f32 = 7.0;

/// Glyph advance at size 8 (monospace: glyph_w=5, +1 spacing).
const CHAR_ADVANCE_BASE: f32 = 6.0;

/// Document layout handler.
pub struct Mtd1Handler {
    // Layout cursor
    origin_x: f32,
    origin_y: f32,
    x: f32,
    y: f32,
    viewport_w: f32,
    viewport_h: f32,
    padding: f32,
    // Text style
    color: [f32; 4],
    // Grid state
    grid_cols: u32,
    grid_gap: f32,
    grid_col: u32,
    grid_item: u32,
    // Card state
    card_x: f32,
    card_y: f32,
    card_w: f32,
    card_h: f32,
    in_card: bool,
    // Outputs
    pub sdf_draws: Vec<SdfDrawCmd>,
    pub strings: Vec<String>,
}

impl Mtd1Handler {
    pub fn new() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            x: 0.0,
            y: 0.0,
            viewport_w: 1000.0,
            viewport_h: 800.0,
            padding: 16.0,
            color: [0.8, 0.8, 0.85, 1.0],
            grid_cols: 1,
            grid_gap: 12.0,
            grid_col: 0,
            grid_item: 0,
            card_x: 0.0,
            card_y: 0.0,
            card_w: 0.0,
            card_h: 0.0,
            in_card: false,
            sdf_draws: Vec::new(),
            strings: Vec::new(),
        }
    }

    /// Take the outputs, consuming the handler.
    pub fn into_outputs(self) -> (Vec<SdfDrawCmd>, Vec<String>) {
        (self.sdf_draws, self.strings)
    }

    // ── Public API (direct Rust usage, no VM) ──────────────────────

    pub fn doc_begin(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.origin_x = x;
        self.origin_y = y;
        self.viewport_w = w;
        self.viewport_h = h;
        self.x = x + self.padding;
        self.y = y + self.padding;

        // Background
        self.push_slab(x, y, w, h, 16.0, [0.05, 0.05, 0.08, 0.97]);
        // Border
        self.push_slab(x + 1.0, y + 1.0, w - 2.0, h - 2.0, 15.0, [0.15, 0.15, 0.25, 0.4]);
        // Inner fill
        self.push_slab(x + 2.0, y + 2.0, w - 4.0, h - 4.0, 14.0, [0.06, 0.06, 0.10, 0.98]);
    }

    pub fn doc_end(&mut self) {
        // Nothing to flush
    }

    pub fn heading(&mut self, text: &str, level: u32) {
        let size = match level {
            1 => SIZE_H1,
            2 => SIZE_H2,
            _ => SIZE_H3,
        };
        let color = [1.0, 1.0, 1.0, 1.0];
        let container_w = self.container_width();
        let max_chars = self.max_chars(container_w, size);
        let display = truncate(text, max_chars);
        self.push_text(&display, self.x, self.y, size, color);
        self.y += size + 6.0;
    }

    pub fn text_block(&mut self, text: &str) {
        let container_w = self.container_width();
        let max_chars = self.max_chars(container_w, SIZE_BODY);
        let lines = wordwrap(text, max_chars);
        let line_h = SIZE_BODY + 3.0;
        for line in &lines {
            if self.y + line_h > self.origin_y + self.viewport_h - self.padding {
                break; // clip to viewport
            }
            self.push_text(line, self.x, self.y, SIZE_BODY, self.color);
            self.y += line_h;
        }
    }

    pub fn label(&mut self, text: &str) {
        let container_w = self.container_width();
        let max_chars = self.max_chars(container_w, SIZE_LABEL);
        let display = truncate(text, max_chars);
        self.push_text(&display, self.x, self.y, SIZE_LABEL, [0.45, 0.65, 0.45, 0.8]);
    }

    pub fn label_right(&mut self, text: &str) {
        let advance = char_advance(SIZE_LABEL);
        let text_w = text.len() as f32 * advance;
        let right_x = if self.in_card {
            self.card_x + self.card_w - self.padding - text_w
        } else {
            self.origin_x + self.viewport_w - self.padding - text_w
        };
        self.push_text(text, right_x, self.y, SIZE_LABEL, [0.45, 0.65, 0.45, 0.8]);
    }

    pub fn tag(&mut self, text: &str) {
        let display = format!("[{}]", text);
        let advance = char_advance(SIZE_TAG);
        let tag_w = display.len() as f32 * advance;

        // Check if tag fits on current line
        let container_end = if self.in_card {
            self.card_x + self.card_w - self.padding
        } else {
            self.origin_x + self.viewport_w - self.padding
        };
        if self.x + tag_w > container_end {
            // Wrap to next line
            self.x = if self.in_card { self.card_x + self.padding } else { self.origin_x + self.padding };
            self.y += SIZE_TAG + 3.0;
        }

        self.push_text(&display, self.x, self.y, SIZE_TAG, [0.35, 0.55, 0.85, 0.7]);
        self.x += tag_w + 4.0; // 4dp gap between tags
    }

    pub fn tags_end(&mut self) {
        self.y += SIZE_TAG + 5.0;
        self.x = if self.in_card { self.card_x + self.padding } else { self.origin_x + self.padding };
    }

    pub fn divider(&mut self) {
        let left = if self.in_card { self.card_x + self.padding } else { self.origin_x + self.padding };
        let w = self.container_width();
        self.sdf_draws.push(SdfDrawCmd {
            pos: [left, self.y],
            size: [w, 1.0],
            color: [0.2, 0.2, 0.3, 0.4],
            params: [matterstream_common::DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
        });
        self.y += 6.0;
    }

    pub fn spacer(&mut self, height: f32) {
        self.y += height;
    }

    pub fn grid_begin(&mut self, cols: u32, gap: f32) {
        self.grid_cols = cols.max(1);
        self.grid_gap = gap;
        self.grid_col = 0;
        self.grid_item = 0;
    }

    pub fn grid_end(&mut self) {
        self.grid_cols = 1;
        self.grid_gap = 0.0;
    }

    pub fn card_begin(&mut self, w: f32, h: f32) {
        let col = self.grid_item % self.grid_cols;
        let row = self.grid_item / self.grid_cols;

        let cx = self.origin_x + self.padding + (col as f32) * (w + self.grid_gap);
        let cy = self.y + (row as f32) * (h + self.grid_gap);

        self.card_x = cx;
        self.card_y = cy;
        self.card_w = w;
        self.card_h = h;
        self.in_card = true;

        // Card background
        self.push_slab(cx, cy, w, h, 10.0, [0.10, 0.10, 0.16, 0.9]);

        // Position cursor inside card
        self.x = cx + self.padding;
        self.y = cy + self.padding;

        self.grid_col = col;
    }

    pub fn card_end(&mut self) {
        self.in_card = false;
        self.grid_item += 1;

        // If we completed a row, advance Y past the card
        if self.grid_item % self.grid_cols == 0 {
            self.y = self.card_y + self.card_h + self.grid_gap;
        }

        // Reset x to left margin
        self.x = self.origin_x + self.padding;
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
    }

    pub fn set_padding(&mut self, padding: f32) {
        self.padding = padding;
    }

    // ── Dismiss hint ───────────────────────────────────────────────

    pub fn dismiss_hint(&mut self) {
        let text = "[tap to dismiss]";
        let advance = char_advance(SIZE_LABEL);
        let text_w = text.len() as f32 * advance;
        let rx = self.origin_x + self.viewport_w - self.padding - text_w;
        self.push_text(text, rx, self.origin_y + self.padding, SIZE_LABEL, [0.4, 0.4, 0.5, 0.6]);
    }

    // ── Internal helpers ───────────────────────────────────────────

    fn container_width(&self) -> f32 {
        if self.in_card {
            self.card_w - self.padding * 2.0
        } else {
            self.viewport_w - self.padding * 2.0
        }
    }

    fn max_chars(&self, container_w: f32, text_size: f32) -> usize {
        let advance = char_advance(text_size);
        (container_w / advance) as usize
    }

    fn push_slab(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: [f32; 4]) {
        self.sdf_draws.push(SdfDrawCmd {
            pos: [x, y],
            size: [w, h],
            color,
            params: [matterstream_common::DRAW_TYPE_SLAB, radius, 0.0, 0.0],
        });
    }

    fn push_text(&mut self, text: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        if text.is_empty() { return; }
        let advance = char_advance(size);
        let w = text.len() as f32 * advance;
        let idx = self.strings.len();
        self.strings.push(text.to_string());
        self.sdf_draws.push(SdfDrawCmd {
            pos: [x, y],
            size: [w, size],
            color,
            params: [matterstream_common::DRAW_TYPE_TEXT, 0.0, 0.0, idx as f32],
        });
    }
}

impl Default for Mtd1Handler {
    fn default() -> Self { Self::new() }
}

// ── OR page dispatch (for VM usage) ────────────────────────────────────

impl OrPageHandler for Mtd1Handler {
    fn dispatch(
        &mut self,
        sub_op: u8,
        vm: &mut VmHandle,
        _arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        match sub_op {
            0x00 => { // DocBegin: w, h (as u32)
                let h = vm.pop_u32()? as f32;
                let w = vm.pop_u32()? as f32;
                self.doc_begin(0.0, 0.0, w, h);
            }
            0x01 => { // DocEnd
                self.doc_end();
            }
            0x02 => { // Heading: str_idx, level
                let level = vm.pop_u32()?;
                let idx = vm.pop_u32()?;
                let text = vm.resolve_str(idx)?;
                self.heading(&text, level);
            }
            0x03 => { // TextBlock: str_idx
                let idx = vm.pop_u32()?;
                let text = vm.resolve_str(idx)?;
                self.text_block(&text);
            }
            0x04 => { // Tag: str_idx
                let idx = vm.pop_u32()?;
                let text = vm.resolve_str(idx)?;
                self.tag(&text);
            }
            0x05 => { // TagsEnd
                self.tags_end();
            }
            0x06 => { // Divider
                self.divider();
            }
            0x07 => { // Spacer: height
                let h = vm.pop_u32()? as f32;
                self.spacer(h);
            }
            0x08 => { // CardBegin: w, h (as u32)
                let h = vm.pop_u32()? as f32;
                let w = vm.pop_u32()? as f32;
                self.card_begin(w, h);
            }
            0x09 => { // CardEnd
                self.card_end();
            }
            0x0A => { // SetColor: rgba packed
                let rgba = vm.pop_u32()?;
                self.set_color(matterstream_common::color_u32_to_f32(rgba));
            }
            0x0B => { // SetPadding: pad (as u32)
                let pad = vm.pop_u32()? as f32;
                self.set_padding(pad);
            }
            0x0C => { // Label: str_idx
                let idx = vm.pop_u32()?;
                let text = vm.resolve_str(idx)?;
                self.label(&text);
            }
            0x0D => { // GridBegin: cols, gap (as u32)
                let gap = vm.pop_u32()? as f32;
                let cols = vm.pop_u32()?;
                self.grid_begin(cols, gap);
            }
            0x0E => { // GridEnd
                self.grid_end();
            }
            _ => {}
        }
        Ok(())
    }

    fn gas_cost(&self, _sub_op: u8) -> u64 { 50 }

    fn as_any(self: Box<Self>) -> Box<dyn Any> { self }
    fn as_any_ref(&self) -> &dyn Any { self }
}

// ── Text layout helpers ────────────────────────────────────────────────

/// Character advance width at a given text size.
fn char_advance(text_size: f32) -> f32 {
    CHAR_ADVANCE_BASE * (text_size / 8.0)
}

// Re-export from matterstream-common
pub use matterstream_common::truncate_str as truncate;
pub use matterstream_common::wordwrap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordwrap_basic() {
        let lines = wordwrap("hello world this is a test", 12);
        assert_eq!(lines, vec!["hello world", "this is a", "test"]);
    }

    #[test]
    fn wordwrap_long_word() {
        let lines = wordwrap("supercalifragilisticexpialidocious", 10);
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn wordwrap_fits() {
        let lines = wordwrap("short", 20);
        assert_eq!(lines, vec!["short"]);
    }

    #[test]
    fn doc_emits_draws() {
        let mut h = Mtd1Handler::new();
        h.doc_begin(0.0, 0.0, 400.0, 300.0);
        h.heading("Test Document", 1);
        h.text_block("This is body text that should be word-wrapped to fit.");
        h.doc_end();
        let (draws, strings) = h.into_outputs();
        assert!(!draws.is_empty());
        assert!(!strings.is_empty());
        assert!(strings.iter().any(|s| s.contains("Test Document")));
    }

    #[test]
    fn grid_layout() {
        let mut h = Mtd1Handler::new();
        h.doc_begin(0.0, 0.0, 1200.0, 800.0);
        h.grid_begin(3, 10.0);
        for i in 0..6 {
            h.card_begin(380.0, 100.0);
            h.heading(&format!("Card {}", i + 1), 3);
            h.card_end();
        }
        h.grid_end();
        h.doc_end();
        let (draws, strings) = h.into_outputs();
        // 6 cards: each has 1 slab bg + 1 heading text = 12 + doc bg (3) = 15
        assert!(draws.len() >= 15, "expected >= 15 draws, got {}", draws.len());
        assert_eq!(strings.len(), 6); // 6 heading strings
    }

    #[test]
    fn tags_inline() {
        let mut h = Mtd1Handler::new();
        h.doc_begin(0.0, 0.0, 400.0, 300.0);
        h.tag("rust");
        h.tag("async");
        h.tag("wgpu");
        h.tags_end();
        h.doc_end();
        let (draws, strings) = h.into_outputs();
        assert!(strings.iter().any(|s| s == "[rust]"));
        assert!(strings.iter().any(|s| s == "[async]"));
        assert!(strings.iter().any(|s| s == "[wgpu]"));
    }
}
