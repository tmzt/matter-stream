use crate::ast_tsx::{TsxAttributes, TsxElement, TsxFragment, TsTypeValue, TsTypeDef};
use dashmap::DashMap;
use std::any::Any;

pub type MtsmSlotId = u64;
pub type MtsmTimestamp = u64;

/// Binder entry tracks constants, late-bound identifiers, and special variants.
#[derive(Debug, Clone)]
pub enum BinderEntry {
    Constant(TsTypeValue),
    LateBound(Option<TsTypeDef>),
    Special, // Placeholder: special entries are registered but payloads live in MtsmObject
}

/// Thread-safe Binder for tracking top-level identifiers discovered while parsing.
pub struct Binder {
    pub map: DashMap<String, BinderEntry>,
}

impl Binder {
    pub fn new() -> Self {
        Binder { map: DashMap::new() }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    pub fn insert_constant(&self, name: &str, val: TsTypeValue) -> Result<(), String> {
        if self.map.contains_key(name) {
            Err(format!("Identifier '{}' already defined (shadowing not allowed)", name))
        } else {
            self.map.insert(name.to_string(), BinderEntry::Constant(val));
            Ok(())
        }
    }

    pub fn insert_latebound(&self, name: &str, ttype: Option<TsTypeDef>) -> Result<(), String> {
        if self.map.contains_key(name) {
            Err(format!("Identifier '{}' already defined (shadowing not allowed)", name))
        } else {
            self.map.insert(name.to_string(), BinderEntry::LateBound(ttype));
            Ok(())
        }
    }

    pub fn insert_special(&self, name: &str) -> Result<(), String> {
        if self.map.contains_key(name) {
            Err(format!("Identifier '{}' already defined (shadowing not allowed)", name))
        } else {
            self.map.insert(name.to_string(), BinderEntry::Special);
            Ok(())
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
