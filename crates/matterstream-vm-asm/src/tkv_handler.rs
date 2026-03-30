//! TKV arena UserCallHandler — core functionality for external component props.

use matterstream_vm::user_call_handler::UserCallHandler;
use matterstream_vm::rpn::{RpnError, RpnValue};
use matterstream_vm::vm_handle::VmHandle;
use matterstream_vm::addressing::{
    TkvFixedEntry, TkvKey, TkvType, StrRefDisc,
    tkv_ops::*,
    ova::Ova,
};
use matterstream_vm_arena::TripleArena;
use std::any::Any;

/// Load compiled TKV templates into the nursery arena and register in the VM.
/// Call after VM creation and before execution.
pub fn load_templates(
    vm: &mut matterstream_vm::rpn::RpnVm,
    arenas: &mut TripleArena,
    templates: &[crate::TkvTemplate],
) {
    for tmpl in templates {
        // Serialize entries to bytes: [u32 count] + [16-byte entries...]
        let count = tmpl.entries.len();
        let size = TKV_HEADER_SIZE + count * TKV_ENTRY_SIZE;
        let mut data = vec![0u8; size];
        data[0..4].copy_from_slice(&(count as u32).to_le_bytes());
        for (i, entry) in tmpl.entries.iter().enumerate() {
            let offset = TKV_HEADER_SIZE + i * TKV_ENTRY_SIZE;
            data[offset..offset + TKV_ENTRY_SIZE].copy_from_slice(&entry.to_bytes());
        }

        // Allocate in nursery (immutable)
        match arenas.alloc_nursery(size) {
            Ok(ova) => {
                if arenas.write(ova, &data).is_ok() {
                    vm.tkv_static_templates.push(ova);
                }
            }
            Err(e) => {
                // silently skip — nursery may be full
            }
        }
    }
}

pub struct TkvArenaHandler;

impl TkvArenaHandler {
    pub fn new() -> Self { Self }
}

impl UserCallHandler for TkvArenaHandler {
    fn dispatch(
        &mut self,
        sub: u64,
        vm: &mut VmHandle,
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        let op = TkvOp::from_u64(sub).ok_or(RpnError::TypeMismatch)?;
        match op {
            TkvOp::Clone => {
                let template_idx = vm.pop_u32()? as usize;
                let template_ova = vm.tkv_static_templates()
                    .get(template_idx)
                    .copied()
                    .ok_or(RpnError::TypeMismatch)?;
                let data = arenas.read(template_ova)
                    .map_err(|_| RpnError::TypeMismatch)?
                    .to_vec();
                let ova = arenas.alloc_staging(TKV_SLOT_SIZE)
                    .map_err(|_| RpnError::StackOverflow)?;
                let mut buf = vec![0u8; TKV_SLOT_SIZE];
                let n = data.len().min(TKV_SLOT_SIZE);
                buf[..n].copy_from_slice(&data[..n]);
                arenas.write(ova, &buf).map_err(|_| RpnError::StackOverflow)?;
                vm.push(RpnValue::Ova(ova))?;
            }

            TkvOp::New => {
                let ova = arenas.alloc_staging(TKV_SLOT_SIZE)
                    .map_err(|_| RpnError::StackOverflow)?;
                let buf = vec![0u8; TKV_SLOT_SIZE];
                arenas.write(ova, &buf).map_err(|_| RpnError::StackOverflow)?;
                vm.push(RpnValue::Ova(ova))?;
            }

            TkvOp::Set => {
                // Pop value (type determined by existing entry), ordinal, ova
                let value_raw = vm.pop()?;
                let ordinal = vm.pop_u32()? as usize;
                let ova = vm.pop_ova()?;
                let mut entry = read_entry(arenas, ova, ordinal)?;
                match entry.value_type {
                    t if t == TkvType::String as u8 => {
                        let str_id = match value_raw {
                            RpnValue::U32(v) => v,
                            _ => return Err(RpnError::TypeMismatch),
                        };
                        let mut val = [0u8; 8];
                        val[0] = StrRefDisc::StringTable as u8;
                        val[1..5].copy_from_slice(&str_id.to_le_bytes());
                        entry.value = val;
                    }
                    t if t == TkvType::Integer as u8 => {
                        let v = match value_raw {
                            RpnValue::U64(v) => v,
                            RpnValue::U32(v) => v as u64,
                            _ => return Err(RpnError::TypeMismatch),
                        };
                        entry.value = v.to_le_bytes();
                    }
                    t if t == TkvType::Boolean as u8 => {
                        let v = match value_raw {
                            RpnValue::U32(v) => v,
                            RpnValue::U64(v) => v as u32,
                            _ => return Err(RpnError::TypeMismatch),
                        };
                        let mut val = [0u8; 8];
                        val[0] = if v != 0 { 1 } else { 0 };
                        entry.value = val;
                    }
                    _ => return Err(RpnError::TypeMismatch),
                }
                write_entry(arenas, ova, ordinal, &entry)?;
                vm.push(RpnValue::Ova(ova))?;
            }

            TkvOp::Get => {
                let ordinal = vm.pop_u32()? as usize;
                let ova = vm.pop_ova()?;
                let entry = read_entry(arenas, ova, ordinal)?;
                push_entry_value(vm, &entry)?;
            }

            TkvOp::FindKey => {
                let key_raw = vm.pop_u32()?;
                let ova = vm.pop_ova()?;
                let target = TkvKey(key_raw).sort_key();
                let idx = binary_search_sort_key(arenas, ova, target)?;
                vm.push(RpnValue::U32(idx.unwrap_or(u32::MAX as usize) as u32))?;
            }

            TkvOp::FindStrKey => {
                let str_id = vm.pop_u32()?;
                let ova = vm.pop_ova()?;
                let count = read_count(arenas, ova)?;
                let mut found = u32::MAX;
                for i in 0..count {
                    let e = read_entry(arenas, ova, i)?;
                    if e.key_str_disc == StrRefDisc::StringTable as u8 && e.key_str_idx == str_id as u16 {
                        found = i as u32;
                        break;
                    }
                }
                vm.push(RpnValue::U32(found))?;
            }

            TkvOp::AddKey => {
                let value_raw = vm.pop()?;
                let key_name_idx = vm.pop_u32()? as u16;
                let key_name_disc = vm.pop_u32()? as u8;
                let key_raw = vm.pop_u32()?;
                let ova = vm.pop_ova()?;
                let key = TkvKey(key_raw);
                let vtype = key.type_tag();
                let value = encode_value_from_stack(vtype, &value_raw)?;
                let new_entry = TkvFixedEntry {
                    key_path: key_raw,
                    value_type: vtype,
                    value,
                    key_str_disc: key_name_disc,
                    key_str_idx: key_name_idx,
                };
                add_entry_sorted(arenas, ova, &new_entry)?;
                vm.push(RpnValue::Ova(ova))?;
            }

            TkvOp::AddStrKey => {
                let value_raw = vm.pop()?;
                let value_type = vm.pop_u32()? as u8;
                let str_id = vm.pop_u32()?;
                let parent_key_raw = vm.pop_u32()?;
                let ova = vm.pop_ova()?;
                // Find next available child segment under parent
                let parent = TkvKey(parent_key_raw);
                let next_seg = find_next_child_segment(arenas, ova, &parent)?;
                let child_key = parent.child(next_seg, TkvType::from_u8(value_type).unwrap_or(TkvType::Null));
                let value = encode_value_from_stack(value_type, &value_raw)?;
                let new_entry = TkvFixedEntry {
                    key_path: child_key.raw(),
                    value_type,
                    value,
                    key_str_disc: StrRefDisc::StringTable as u8,
                    key_str_idx: str_id as u16,
                };
                add_entry_sorted(arenas, ova, &new_entry)?;
                vm.push(RpnValue::Ova(ova))?;
            }

            TkvOp::Seal => {
                let ova = vm.pop_ova()?;
                let count = read_count(arenas, ova)?;
                let actual_size = TKV_HEADER_SIZE + count * TKV_ENTRY_SIZE;
                let data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?.to_vec();
                let nursery_ova = arenas.alloc_nursery(actual_size)
                    .map_err(|_| RpnError::StackOverflow)?;
                arenas.write(nursery_ova, &data[..actual_size])
                    .map_err(|_| RpnError::StackOverflow)?;
                vm.push(RpnValue::Ova(nursery_ova))?;
            }

            TkvOp::Count => {
                let ova = vm.pop_ova()?;
                let count = read_count(arenas, ova)?;
                vm.push(RpnValue::U32(count as u32))?;
            }
        }
        Ok(())
    }

    fn gas_cost(&self, sub: u64) -> u64 {
        match TkvOp::from_u64(sub) {
            Some(TkvOp::AddKey | TkvOp::AddStrKey) => 500,
            Some(TkvOp::FindKey) => 200,
            Some(TkvOp::FindStrKey) => 300, // linear scan
            _ => 100,
        }
    }

    fn as_any(self: Box<Self>) -> Box<dyn Any> { self }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn pop_u128(vm: &mut VmHandle) -> Result<u128, RpnError> {
    let v = vm.pop()?;
    match v {
        RpnValue::Fqa(f) => Ok(f.value()),
        RpnValue::U64(lo) => {
            let hi = vm.pop_u64()?;
            Ok(((hi as u128) << 64) | (lo as u128))
        }
        RpnValue::U32(lo) => Ok(lo as u128),
        _ => Err(RpnError::TypeMismatch),
    }
}

fn read_count(arenas: &TripleArena, ova: Ova) -> Result<usize, RpnError> {
    let data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?;
    if data.len() < TKV_HEADER_SIZE { return Ok(0); }
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize)
}

fn read_entry(arenas: &TripleArena, ova: Ova, ordinal: usize) -> Result<TkvFixedEntry, RpnError> {
    let data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?;
    let offset = TKV_HEADER_SIZE + ordinal * TKV_ENTRY_SIZE;
    if offset + TKV_ENTRY_SIZE > data.len() { return Err(RpnError::TypeMismatch); }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&data[offset..offset + TKV_ENTRY_SIZE]);
    Ok(TkvFixedEntry::from_bytes(&bytes))
}

fn write_entry(arenas: &mut TripleArena, ova: Ova, ordinal: usize, entry: &TkvFixedEntry) -> Result<(), RpnError> {
    let offset = TKV_HEADER_SIZE + ordinal * TKV_ENTRY_SIZE;
    let mut data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?.to_vec();
    if offset + TKV_ENTRY_SIZE > data.len() { return Err(RpnError::TypeMismatch); }
    data[offset..offset + TKV_ENTRY_SIZE].copy_from_slice(&entry.to_bytes());
    arenas.write(ova, &data).map_err(|_| RpnError::StackOverflow)
}

fn binary_search_sort_key(arenas: &TripleArena, ova: Ova, target: u32) -> Result<Option<usize>, RpnError> {
    let count = read_count(arenas, ova)?;
    let data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?;
    let mut lo = 0usize;
    let mut hi = count;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let off = TKV_HEADER_SIZE + mid * TKV_ENTRY_SIZE;
        let k = u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]);
        let sk = TkvKey(k).sort_key();
        if sk < target { lo = mid + 1; }
        else if sk > target { hi = mid; }
        else { return Ok(Some(mid)); }
    }
    Ok(None)
}

fn push_entry_value(vm: &mut VmHandle, entry: &TkvFixedEntry) -> Result<(), RpnError> {
    match entry.value_type {
        t if t == TkvType::Integer as u8 => {
            vm.push(RpnValue::U64(u64::from_le_bytes(entry.value)))?;
        }
        t if t == TkvType::Boolean as u8 => {
            vm.push(RpnValue::U32(if entry.value[0] != 0 { 1 } else { 0 }))?;
        }
        t if t == TkvType::String as u8 => {
            let idx = u32::from_le_bytes([entry.value[1], entry.value[2], entry.value[3], entry.value[4]]);
            let disc = entry.value[0];
            if disc == StrRefDisc::StringTable as u8 {
                vm.push(RpnValue::U32(idx))?;
            } else {
                vm.push(RpnValue::U32(idx | 0x8000_0000))?; // high bit = runtime string
            }
        }
        _ => vm.push(RpnValue::U32(0))?,
    }
    Ok(())
}

fn encode_value_from_stack(value_type: u8, value: &RpnValue) -> Result<[u8; 8], RpnError> {
    let mut buf = [0u8; 8];
    match value_type {
        t if t == TkvType::String as u8 => {
            let str_id = match value {
                RpnValue::U32(v) => *v,
                _ => return Err(RpnError::TypeMismatch),
            };
            buf[0] = StrRefDisc::StringTable as u8;
            buf[1..5].copy_from_slice(&str_id.to_le_bytes());
        }
        t if t == TkvType::Integer as u8 => {
            let v = match value {
                RpnValue::U64(v) => *v,
                RpnValue::U32(v) => *v as u64,
                _ => return Err(RpnError::TypeMismatch),
            };
            buf = v.to_le_bytes();
        }
        t if t == TkvType::Boolean as u8 => {
            let v = match value {
                RpnValue::U32(v) => *v,
                RpnValue::U64(v) => *v as u32,
                _ => return Err(RpnError::TypeMismatch),
            };
            buf[0] = if v != 0 { 1 } else { 0 };
        }
        _ => {}
    }
    Ok(buf)
}

fn add_entry_sorted(arenas: &mut TripleArena, ova: Ova, new_entry: &TkvFixedEntry) -> Result<(), RpnError> {
    let mut data = arenas.read(ova).map_err(|_| RpnError::TypeMismatch)?.to_vec();
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let target = TkvKey(new_entry.key_path).sort_key();

    let mut lo = 0usize;
    let mut hi = count;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let off = TKV_HEADER_SIZE + mid * TKV_ENTRY_SIZE;
        let k = u32::from_le_bytes([data[off], data[off+1], data[off+2], data[off+3]]);
        if TkvKey(k).sort_key() < target { lo = mid + 1; } else { hi = mid; }
    }

    let insert_off = TKV_HEADER_SIZE + lo * TKV_ENTRY_SIZE;
    let end_off = TKV_HEADER_SIZE + count * TKV_ENTRY_SIZE;
    if end_off + TKV_ENTRY_SIZE > data.len() { return Err(RpnError::StackOverflow); }
    data.copy_within(insert_off..end_off, insert_off + TKV_ENTRY_SIZE);
    data[insert_off..insert_off + TKV_ENTRY_SIZE].copy_from_slice(&new_entry.to_bytes());
    let new_count = (count + 1) as u32;
    data[0..4].copy_from_slice(&new_count.to_le_bytes());
    arenas.write(ova, &data).map_err(|_| RpnError::StackOverflow)
}

fn find_next_child_segment(arenas: &TripleArena, ova: Ova, parent: &TkvKey) -> Result<u8, RpnError> {
    let count = read_count(arenas, ova)?;
    let parent_depth = parent.prefix_len();
    let mut max_seg = 0u8;
    for i in 0..count {
        let e = read_entry(arenas, ova, i)?;
        let k = TkvKey(e.key_path);
        if k.has_prefix(*parent) && k.prefix_len() == parent_depth + 1 {
            let seg = k.segment(parent_depth);
            if seg >= max_seg { max_seg = seg + 1; }
        }
    }
    if max_seg > 7 { return Err(RpnError::StackOverflow); } // no more child slots
    Ok(max_seg)
}
