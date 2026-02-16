use crate::ast_tsx::{TsxAttributes, TsxElement, TsxFragment, TsTypeValue, TsTypeDef};
use dashmap::DashMap;
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};

pub type MtsmSlotId = u64;
pub type MtsmTimestamp = u64;

/// Package handle type (namespace resolver id + serial)
pub type MtsmPackageHandle = u64;

/// Simple wrapper handle for bindings (wraps a slot id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MtsmBindHandle(pub MtsmSlotId);

/// Trait for package registries. Implemented by concrete registries in other crates.
pub trait MtsmPackageRegistry: Send + Sync {
    fn get_namespace_handle(&self, namespace: &str) -> Option<MtsmPackageHandle>;
    fn resolve_full_import_path(&self, import_path: &str) -> Option<Box<dyn crate::ast_hook::MtsmExecFunctionalComponent>>;
}

/// Binder entry tracks constants, late-bound identifiers, and special variants.
#[derive(Debug, Clone)]
pub enum BinderEntry {
    Constant(TsTypeValue, Option<crate::ast_tsx::SourceLoc>, MtsmBindHandle),
    LateBound(Option<TsTypeDef>, Option<crate::ast_tsx::SourceLoc>, MtsmBindHandle),
    Special(Option<crate::ast_tsx::SourceLoc>, MtsmBindHandle), // Placeholder: special entries are registered but payloads live in MtsmObject
}

/// Thread-safe Binder for tracking top-level identifiers discovered while parsing.
pub struct Binder {
    pub map: DashMap<smol_str::SmolStr, BinderEntry>,
    next_slot: AtomicU64,
}

impl Binder {
    pub fn new() -> Self {
        Binder { map: DashMap::new(), next_slot: AtomicU64::new(1) }
    }

    pub fn contains(&self, name: &str) -> bool {
        let key = smol_str::SmolStr::new(name);
        self.map.contains_key(&key)
    }

    fn alloc_handle(&self) -> MtsmBindHandle {
        let id = self.next_slot.fetch_add(1, Ordering::SeqCst);
        MtsmBindHandle(id)
    }

    pub fn insert_constant(&self, name: &str, val: TsTypeValue, loc: Option<crate::ast_tsx::SourceLoc>) -> Result<MtsmBindHandle, String> {
        let key = smol_str::SmolStr::new(name);
        if self.map.contains_key(&key) {
            Err(format!("Identifier '{}' already defined (shadowing not allowed)", name))
        } else {
            let handle = self.alloc_handle();
            self.map.insert(key, BinderEntry::Constant(val, loc, handle));
            Ok(handle)
        }
    }

    pub fn insert_latebound(&self, name: &str, ttype: Option<TsTypeDef>, loc: Option<crate::ast_tsx::SourceLoc>) -> Result<MtsmBindHandle, String> {
        let key = smol_str::SmolStr::new(name);
        if self.map.contains_key(&key) {
            Err(format!("Identifier '{}' already defined (shadowing not allowed)", name))
        } else {
            let handle = self.alloc_handle();
            self.map.insert(key, BinderEntry::LateBound(ttype, loc, handle));
            Ok(handle)
        }
    }

    pub fn insert_special(&self, name: &str, loc: Option<crate::ast_tsx::SourceLoc>) -> Result<MtsmBindHandle, String> {
        let key = smol_str::SmolStr::new(name);
        if self.map.contains_key(&key) {
            Err(format!("Identifier '{}' already defined (shadowing not allowed)", name))
        } else {
            let handle = self.alloc_handle();
            self.map.insert(key, BinderEntry::Special(loc, handle));
            Ok(handle)
        }
    }

    pub fn insert_anonymous(&self) -> MtsmBindHandle {
        // create an anonymous name and insert mapping
        let id = self.next_slot.fetch_add(1, Ordering::SeqCst);
        let anon_name = format!("__anon_{}", id);
        let key = smol_str::SmolStr::new(anon_name);
        let handle = MtsmBindHandle(id);
        // anonymous latebound with no type/loc
        self.map.insert(key, BinderEntry::LateBound(None, None, handle));
        handle
    }

    pub fn get_handle(&self, name: &str) -> Option<MtsmBindHandle> {
        let key = smol_str::SmolStr::new(name);
        if let Some(entry) = self.map.get(&key) {
            match &*entry {
                BinderEntry::Constant(_, _, h) => Some(*h),
                BinderEntry::LateBound(_, _, h) => Some(*h),
                BinderEntry::Special(_, h) => Some(*h),
            }
        } else {
            None
        }
    }
}


/// Marker trait for types that can be transmitted from TypeScript source to shader.
pub trait TsShaderTransmissible: 'static + Send + Sync {}

/// A non-generic trait that all MtsmBinding<T> must implement.
pub trait AnyMtsmBinding: 'static + Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A non-generic trait that all MtsmHook<T> must implement.
pub trait AnyMtsmHook: 'static + Send + Sync {
    fn as_any_hook(&self) -> &dyn Any;
    fn as_any_hook_mut(&mut self) -> &mut dyn Any;
}

#[derive(Debug)]
pub struct TsxElementContext {
    pub attributes: TsxAttributes,
    pub children: Option<TsxFragment>,
}

pub trait MtsmTsxFunctionalComponent: 'static + Send + Sync {
    fn render(&self, context: TsxElementContext) -> TsxFragment;
}

pub trait MtsmExecFunctionalComponent: 'static + Send + Sync {
    fn execute(&self, context: TsxElementContext) -> TsxFragment; // For now, returns TsxFragment
}

/*
 * For reference only
 *

 #[repr(C)]
pub struct MtsmSlotIdBitfield {
    pub constrained_slot_id: u32,
    pub generation: u32,
}

#[repr(C)]
pub union MtsmSlotIdUnion {
    pub bitfield: MtsmSlotIdBitfield,
    pub raw: u64,
}

*/

pub struct MtsmBoundSlotValue<T: TsShaderTransmissible> {
    pub slot_id: MtsmSlotId,
    pub value: T,
}

pub struct MtsmBoundSlotValueUpdate<T: TsShaderTransmissible> {
    pub slot_id: MtsmSlotId,
    pub update_ts: MtsmTimestamp,
    pub new_value: T,
}

pub trait MtsmActionGetter<T: TsShaderTransmissible> {
    fn get(&self) -> T;
}

pub trait MtsmSlotGetter<T: TsShaderTransmissible> {
    fn getter(&self) -> MtsmBoundSlotValue<T>;
}

pub trait MtsmSlotSetter<T: TsShaderTransmissible> {
    fn setter(&self) -> MtsmBoundSlotValueUpdate<T>;
}

pub trait MtsmActionSetter<T: TsShaderTransmissible> {
    fn set(&self, value: T);
}

pub trait MtsmHook<T: TsShaderTransmissible>: AnyMtsmHook {
    fn getter(&self) -> Option<Box<dyn MtsmActionGetter<T>>>;
    fn setter(&self) -> Option<Box<dyn MtsmActionSetter<T>>>;
}

#[derive(Default)]
pub struct MtsmObject {
    pub data: DashMap<String, MtsmVariant>,
}

pub enum MtsmVariant {
    Primitive(MtsmPrimitive),
    NestedObject(MtsmObject),
    Array(Vec<MtsmVariant>),
    Tsx(TsxElement),
    TsxFragment(TsxFragment),
    TsxFunctionalComponent(Box<dyn MtsmTsxFunctionalComponent>),
    TsxExecFunctionalComponent(Box<dyn MtsmExecFunctionalComponent>),
    Binding(Box<dyn AnyMtsmBinding>), // Updated to use AnyMtsmBinding
    SecureSourceSymbol(MtsmSecureSourceSymbol),
}

pub enum MtsmPrimitive {
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
}

pub enum MtsmBinding<T: TsShaderTransmissible> {
    Hook(Box<dyn MtsmHook<T>>),
}

impl<T: TsShaderTransmissible> AnyMtsmBinding for MtsmBinding<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// Identifier in TS source code
pub type MtsmSourceIdentifier = String;

// Equivalet to ECMA Symbol object
pub type MtsmSourceSymbol = String;

// Represents a Capability()
pub struct MtsmSecureSourceSymbol {
    pub sym: MtsmSourceIdentifier,
    pub package_id: u64,
    pub key: u64,
}

struct MtsmUseStateBindingPair {
    pub getter: MtsmSourceIdentifier,
    pub setter: MtsmSourceIdentifier,
}

struct MtsmUseStateActionBindingPair;

// Commented out as MtsmHook is now generic and requires a concrete T
// impl MtsmHook for MtsmUseStateActionBindingPair {
//     fn getter(&self) -> Option<MtsmActionGetter> {
//         None
//     }

//     fn setter(&self) -> Option<MtsmActionSetter> {
//         None
//     }
// }

struct MtsmUseStateHook {
    pub id: u32,
    pub source_bindings: MtsmUseStateBindingPair,
    pub action_bindings: MtsmUseStateActionBindingPair,
}
