pub type MtsmSlotId = u64;

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

pub struct MtsmBoundSlotValue<T> {
    pub slot_id: MtsmSlotId,
    pub value: T,
}

pub struct MtsmBoundSlotValueUpdate<T> {
    pub slot_id: MtsmSlotId,
    pub update_ts: MtsmTimestamp,
    pub new_value: T,
}

pub trait MtsmActionGetter {
    fn get(&self) -> MtsmVariant;
}

pub trait MtsmSlotGetter {
    fn getter(&self) -> MtsmBoundSlotValue<T>;
}

pub trait MtsmSlotSetter {
    fn setter(&self) -> MtsmBoundSlotValueUpdate<T>;
}

pub trait MtsmActionSetter {
    fn set(&self, value: MtsmVariant);
}

pub trait MtsmHook {
    fn getter(&self) -> Option<MtsmActionGetter>;
    fn setter(&self) -> Option<MtsmActionSetter>;
}

pub struct MtsmObject {
    pub data: DashMap<String, MtsmVariant>,
}

pub enum MtsmVariant {
    Primitive(MtsmPrimitive),
    NestedObject(MtsmObject),
    Array(Vec<MtsmVariant>),
    Tsx(TsxElement),
    TsxFragment(TsxFragment),
    Binding(MtsmBinding),
    SecureSourceSymbol(MtsmSecureSourceSymbol),
}

pub enum MtsmBinding {
    Hook(MtsmHook<any>),
}

pub type MtsmSourceSymbol = String;

// Represents a Symbol() such as a capability
pub struct MtsmSecureSourceSymbol {
    pub sym: MtsmSourceSymbol,
    pub package_id: u64,
    pub key: u64,
}

struct MtsmUseStateBindingPair {
    pub getter: MtsmSourceSymbol,
    pub setter: MtsmSourceSymbol,
}

struct MtsmUseStateActionBindingPair;

impl MtsmHook for MtsmUseStateActionBindingPair {
    fn getter(&self) -> Option<MtsmActionGetter> {
        None
    }

    fn setter(&self) -> Option<MtsmActionSetter> {
        None
    }
}

struct MtsmUseStateHook {
    pub id: u32,
    pub source_bindings: MtsmUseStateBindingPair,
    pub action_bindings: MtsmUseStateActionBindingPair,
}
