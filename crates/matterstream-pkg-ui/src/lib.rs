//! @chitin/ui package — native UI components for MatterStream VM.
//!
//! Provides `MarkdownReadView` as an OID component that renders markdown
//! text with word-wrapping directly into SDF draw commands.
//!
//! Register via `register(vm)` or use `RpnVm::with_packages()` helper.

use std::collections::HashMap;
use matterstream_vm::rpn::{RpnError, RpnValue, VmHandleNative, VmHandleUiRegistrationNative, VmPackage};
use matterstream_vm_addressing::oid::Oid;
use matterstream_vm_addressing::oid_index::OidIndexBuilder;
use matterstream_vm_addressing::{TkvFixedEntry, TkvType, StrRefDisc};
use matterstream_vm_addressing::tkv_ops::{TKV_HEADER_SIZE, TKV_ENTRY_SIZE};
use matterstream_vm_addressing::ova::Ova;
use matterstream_vm_arena::TripleArena;
use matterstream_common::{SdfDrawCmd, DRAW_TYPE_TEXT, DRAW_TYPE_BOX};
use matterstream_common::{wordwrap, truncate_str};

/// Package name for TSX imports.
pub const PACKAGE_NAME: &str = "@chitin/ui";

/// OID for MarkdownReadView component.
pub const OID_MARKDOWN_READ_VIEW: Oid = Oid::PKG_UI.child_const(1);

/// Temporary fixed OVA for passing markdown content via tkv_bank.
/// FourCC 'MKDN' (0x4D4B444E). By convention, host writes content
/// here before VM execution; MarkdownReadView reads it as fallback.
pub const OVA_MARKDOWN_DATA_TEMP: Ova = Ova(0x4D4B444E); // 'MKDN'

const HOOK_MARKDOWN_READ_VIEW: u32 = 0;

/// Font metrics for text layout (monospace 5x8 bitmap font).
const CHAR_ADVANCE: f32 = 6.0; // glyph_w(5) + 1 spacing
const SIZE_H1: f32 = 14.0;
const SIZE_H2: f32 = 12.0;
const SIZE_H3: f32 = 10.0;
const SIZE_BODY: f32 = 8.0;

/// Returns the OID map for @chitin/ui.
pub fn oid_map() -> HashMap<String, Oid> {
    let mut m = HashMap::new();
    m.insert("MarkdownReadView".to_string(), OID_MARKDOWN_READ_VIEW);
    m
}

/// The @chitin/ui package instance.
pub struct UiPackage;

impl VmPackage for UiPackage {
    fn register(&self, handle: &mut VmHandleUiRegistrationNative) {
        let mut builder = OidIndexBuilder::new();
        builder.add_native_hook(OID_MARKDOWN_READ_VIEW, HOOK_MARKDOWN_READ_VIEW);
        handle.add_oid_index(builder.build());
        handle.add_native_hook(markdown_read_view_hook);
    }
}

/// Native hook for MarkdownReadView.
/// Reads layout props (x, y, w, h) from TKV arena (component props).
/// Reads markdown content from `tkv_bank[OVA_MARKDOWN_DATA_TEMP]` by convention.
fn markdown_read_view_hook(
    vm: &mut VmHandleNative,
    arenas: &mut TripleArena,
) -> Result<(), RpnError> {
    let ova = pop_ova(vm)?;
    let props = read_tkv_string_map(vm, arenas, ova)?;

    let x: f32 = props.get("x").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let y: f32 = props.get("y").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let w: f32 = props.get("w").and_then(|v| v.parse().ok()).unwrap_or(400.0);
    let h: f32 = props.get("h").and_then(|v| v.parse().ok()).unwrap_or(300.0);

    // Read content from tkv_bank (set by host before VM execution)
    let content = props.get("content").cloned().unwrap_or_else(|| {
        // Fallback: read from tkv_bank at OVA_MARKDOWN_DATA_TEMP
        read_tkv_bank_string(vm, OVA_MARKDOWN_DATA_TEMP, "content")
            .unwrap_or_default()
    });

    render_markdown(vm, x, y, w, h, &content);
    Ok(())
}

/// Read a string value from tkv_bank by OVA and key name.
fn read_tkv_bank_string(vm: &VmHandleNative, ova: Ova, key: &str) -> Option<String> {
    let entries = vm.tkv_bank_get(ova)?;
    for entry in entries {
        let key_name = if entry.key_str_disc == StrRefDisc::StringTable as u8 {
            vm.resolve_str(entry.key_str_idx as u32).ok()
        } else {
            None
        };
        if key_name.as_deref() != Some(key) { continue; }
        if entry.value_type == TkvType::String as u8 {
            let disc = entry.value[0];
            let idx = u32::from_le_bytes([entry.value[1], entry.value[2], entry.value[3], entry.value[4]]);
            if disc == StrRefDisc::StringTable as u8 {
                return vm.resolve_str(idx).ok();
            }
        }
    }
    None
}

/// Render markdown content as SDF draw commands.
fn render_markdown(vm: &mut VmHandleNative, x: f32, y: f32, w: f32, h: f32, content: &str) {
    let mut cur_y = y;
    let max_y = y + h;
    let line_gap = 3.0;

    for line in content.lines() {
        if cur_y >= max_y { break; }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            cur_y += SIZE_BODY + line_gap;
            continue;
        }

        // Detect heading level
        let (level, text) = if trimmed.starts_with("### ") {
            (3, &trimmed[4..])
        } else if trimmed.starts_with("## ") {
            (2, &trimmed[3..])
        } else if trimmed.starts_with("# ") {
            (1, &trimmed[2..])
        } else {
            (0, trimmed)
        };

        let (size, color) = match level {
            1 => (SIZE_H1, [1.0, 1.0, 1.0, 1.0f32]),
            2 => (SIZE_H2, [0.95, 0.95, 1.0, 1.0]),
            3 => (SIZE_H3, [0.9, 0.9, 0.95, 1.0]),
            _ => (SIZE_BODY, [0.75, 0.75, 0.82, 1.0]),
        };

        let advance = CHAR_ADVANCE * (size / 8.0);
        let max_chars = (w / advance) as usize;

        if level > 0 {
            // Headings: single line, truncated
            let display = truncate_str(text, max_chars);
            emit_text(vm, &display, x, cur_y, size, color);
            cur_y += size + line_gap + 2.0;

            // Underline for h1
            if level == 1 {
                vm.push_sdf_draw(SdfDrawCmd {
                    pos: [x, cur_y],
                    size: [w, 1.0],
                    color: [0.25, 0.25, 0.35, 0.5],
                    params: [DRAW_TYPE_BOX, 0.0, 0.0, 0.0],
                });
                cur_y += 4.0;
            }
        } else {
            // Body text: word-wrapped
            let lines = wordwrap(text, max_chars);
            for wrapped_line in &lines {
                if cur_y >= max_y { break; }
                emit_text(vm, wrapped_line, x, cur_y, size, color);
                cur_y += size + line_gap;
            }
        }
    }
}

fn emit_text(vm: &mut VmHandleNative, text: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
    if text.is_empty() { return; }
    let advance = CHAR_ADVANCE * (size / 8.0);
    let text_w = text.len() as f32 * advance;
    let idx = vm.push_string(text.to_string());
    vm.push_sdf_draw(SdfDrawCmd {
        pos: [x, y],
        size: [text_w, size],
        color,
        params: [DRAW_TYPE_TEXT, 0.0, 0.0, idx as f32],
    });
}

// ── TKV helpers (same pattern as skills_package) ───────────────────────

fn pop_ova(vm: &mut VmHandleNative) -> Result<Ova, RpnError> {
    match vm.pop()? {
        RpnValue::Ova(o) => Ok(o),
        RpnValue::U32(x) => Ok(Ova(x)),
        _ => Err(RpnError::TypeMismatch),
    }
}

fn read_tkv_string_map(
    vm: &VmHandleNative,
    arenas: &TripleArena,
    ova: Ova,
) -> Result<HashMap<String, String>, RpnError> {
    let data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?;
    if data.len() < TKV_HEADER_SIZE { return Ok(HashMap::new()); }
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

    let mut map = HashMap::new();
    for i in 0..count {
        let offset = TKV_HEADER_SIZE + i * TKV_ENTRY_SIZE;
        if offset + TKV_ENTRY_SIZE > data.len() { break; }
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&data[offset..offset + TKV_ENTRY_SIZE]);
        let entry = TkvFixedEntry::from_bytes(&bytes);

        let key = if entry.key_str_disc == StrRefDisc::StringTable as u8 {
            vm.resolve_str(entry.key_str_idx as u32).ok()
        } else {
            None
        };
        let key = match key { Some(k) => k, None => continue };

        let value = match entry.value_type {
            t if t == TkvType::String as u8 => {
                let disc = entry.value[0];
                let idx = u32::from_le_bytes([entry.value[1], entry.value[2], entry.value[3], entry.value[4]]);
                if disc == StrRefDisc::StringTable as u8 {
                    vm.resolve_str(idx).unwrap_or_default()
                } else {
                    String::new()
                }
            }
            t if t == TkvType::Integer as u8 => {
                u64::from_le_bytes(entry.value).to_string()
            }
            _ => continue,
        };

        map.insert(key, value);
    }
    Ok(map)
}
