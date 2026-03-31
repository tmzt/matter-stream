//! MTD1 shared state, OR page handler, and UserCall handler.
//!
//! A single `Mtd1State` owns the shaper, atlas, glyph maps, styles,
//! and Command32 instruction buffer. Both the OR page handler and
//! UserCall handler hold `Arc<Mutex<Mtd1State>>`.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use matterstream_vm::or_page::OrPageHandler;
use matterstream_vm::user_call_handler::UserCallHandler;
use matterstream_vm::rpn::RpnError;
use matterstream_vm::vm_handle::VmHandle;
use matterstream_vm_arena::TripleArena;
use matterstream_mtd1_format::{Command32, BankedStyle};
use matterstream_common::SdfDrawCmd;

use crate::mtd1_format::Mtd1Document;
use crate::mtd1_to_sdf::mtd1_to_sdf;

/// UserCall action op for MTMD operations.
pub const MTMD_ACTION_OP: u64 = 0x4D544D44; // 'MTMD'

/// UserCall sub-ops.
pub const MTMD_INIT: u64 = 0;
pub const MTMD_EMIT: u64 = 1;

/// Shared state between OR page and UserCall handlers.
pub struct Mtd1State {
    pub instructions: Vec<Command32>,
    pub styles: Vec<BankedStyle>,
    pub font_size: f32,
    pub px_range: f32,
    pub gid_to_idx: HashMap<u16, u16>,
    pub std_advances: HashMap<u16, f32>,
    pub glyph_table_u32s: Vec<u32>,
    pub atlas_rgba: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub initialized: bool,
    /// Char buffer from last emit — caller reads this for GPU upload.
    pub char_buffer: Vec<u32>,
}

impl Mtd1State {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            styles: Vec::new(),
            font_size: 20.0,
            px_range: 4.0,
            gid_to_idx: HashMap::new(),
            std_advances: HashMap::new(),
            glyph_table_u32s: Vec::new(),
            atlas_rgba: Vec::new(),
            atlas_width: 0,
            atlas_height: 0,
            initialized: false,
            char_buffer: Vec::new(),
        }
    }

    /// Initialize font pipeline from raw font data bytes.
    pub fn init_font(&mut self, font_data: &[u8]) {
        use matterstream_font::atlas::FontAtlasBuilder;

        let mut builder = FontAtlasBuilder::new(font_data.to_vec(), 48, self.px_range as f64);
        builder.add_ascii();
        let atlas = match builder.build() {
            Ok(a) => a,
            Err(e) => { log::warn!("mtd1: atlas build failed: {e}"); return; }
        };

        self.gid_to_idx.clear();
        self.std_advances.clear();
        self.glyph_table_u32s.clear();
        for (i, e) in atlas.glyphs.iter().enumerate() {
            self.gid_to_idx.insert(e.glyph_id, i as u16);
            self.std_advances.insert(e.glyph_id, e.advance_x);
            self.glyph_table_u32s.extend_from_slice(&e.to_gpu_u32s());
        }

        // RGB → RGBA
        let mut rgba = Vec::with_capacity((atlas.width * atlas.height * 4) as usize);
        for i in 0..(atlas.width * atlas.height) as usize {
            let s = i * 3;
            rgba.push(atlas.pixel_data.get(s).copied().unwrap_or(0));
            rgba.push(atlas.pixel_data.get(s + 1).copied().unwrap_or(0));
            rgba.push(atlas.pixel_data.get(s + 2).copied().unwrap_or(0));
            rgba.push(255);
        }
        self.atlas_rgba = rgba;
        self.atlas_width = atlas.width;
        self.atlas_height = atlas.height;

        // Default styles: 0=body (light), 1=heading (white)
        self.styles.clear();
        self.styles.push(BankedStyle::with_font(0xC0C0D0FF, 0, 0, 0, 1));
        self.styles.push(BankedStyle::with_font(0xFFFFFFFF, 0, 0, 0, 1));

        self.initialized = true;
        log::info!("mtd1: font ready, atlas {}x{}, {} glyphs",
            self.atlas_width, self.atlas_height, self.gid_to_idx.len());
    }

    /// Convert collected instructions to SDF draws. Clears instruction buffer.
    fn emit(&mut self) -> (Vec<SdfDrawCmd>, Vec<u32>) {
        if !self.initialized || self.instructions.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let mut doc = Mtd1Document::new();
        doc.styles = self.styles.clone();
        doc.instructions = std::mem::take(&mut self.instructions);
        let frame = mtd1_to_sdf(&doc, &self.gid_to_idx, &self.std_advances, self.font_size, self.px_range);
        (frame.draws, frame.char_buffer)
    }
}

/// Create paired handlers sharing the same state.
pub fn create_mtd1_handlers(font_data: &[u8]) -> (Mtd1OrPage, MtmdUserCall, Arc<Mutex<Mtd1State>>) {
    let mut state = Mtd1State::new();
    state.init_font(font_data);
    let shared = Arc::new(Mutex::new(state));
    (
        Mtd1OrPage { state: shared.clone() },
        MtmdUserCall { state: shared.clone() },
        shared,
    )
}

// ── OR page handler ────────────────────────────────────────────────────

pub struct Mtd1OrPage {
    state: Arc<Mutex<Mtd1State>>,
}

impl OrPageHandler for Mtd1OrPage {
    fn dispatch(
        &mut self,
        sub_op: u8,
        vm: &mut VmHandle,
        _arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        let mut s = self.state.lock().unwrap();
        match sub_op {
            0x00 => { // draw_glyph
                let glyph_id = vm.pop_u32()? as u16;
                let advance = vm.pop_u32()? as u16;
                s.instructions.push(Command32::draw_glyph(advance.min(4095), glyph_id));
            }
            0x01 => { // set_cursor
                let x = vm.pop_u32()? as i16;
                let y = vm.pop_u32()? as i16;
                s.instructions.push(Command32::set_cursor(y, x));
            }
            0x02 => { // set_style
                let idx = vm.pop_u32()?;
                s.instructions.push(Command32::set_style(idx));
            }
            0x03 => { // draw_shape
                let width = vm.pop_u32()? as u16;
                let height = vm.pop_u32()? as u16;
                s.instructions.push(Command32::draw_shape(height, width));
            }
            0x04 => { // raw Command32
                let raw = vm.pop_u32()?;
                s.instructions.push(Command32(raw));
            }
            _ => {}
        }
        Ok(())
    }

    fn gas_cost(&self, _sub_op: u8) -> u64 { 10 }
    fn as_any(self: Box<Self>) -> Box<dyn Any> { self }
    fn as_any_ref(&self) -> &dyn Any { self }
}

// ── UserCall handler ───────────────────────────────────────────────────

pub struct MtmdUserCall {
    state: Arc<Mutex<Mtd1State>>,
}

impl MtmdUserCall {
    pub fn state(&self) -> &Arc<Mutex<Mtd1State>> { &self.state }
}

impl UserCallHandler for MtmdUserCall {
    fn dispatch(
        &mut self,
        sub_op: u64,
        vm: &mut VmHandle,
        _arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        match sub_op {
            MTMD_INIT => {
                // Already initialized in create_mtd1_handlers
            }
            MTMD_EMIT => {
                let mut s = self.state.lock().unwrap();
                let (draws, char_buffer) = s.emit();
                s.char_buffer = char_buffer;
                drop(s);
                vm.extend_sdf_draws(&draws);
            }
            _ => {}
        }
        Ok(())
    }

    fn gas_cost(&self, _sub_op: u64) -> u64 { 1000 }
    fn as_any(self: Box<Self>) -> Box<dyn Any> { self }
}
