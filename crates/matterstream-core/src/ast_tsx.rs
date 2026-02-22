use dashmap::DashMap;
use smol_str::SmolStr;

use crate::ast_hook::{MtsmSourceSymbol, MtsmBindHandle};

#[derive(Debug)]
pub enum TsxKind {
    Div,
    Span,
    Slab,
    Text,
    Custom(MtsmBindHandle),
}

/// Simple type definitions for TSX attribute typing.
#[derive(Debug, Clone)]
pub enum TsTypeDef {
    Number,
    String,
    Boolean,
    Any,
    NestedObject(DashMap<SmolStr, TsTypeDef>),
}

/// Location in source for an identifier or token.
#[derive(Debug, Clone)]
pub struct SourceLoc {
    pub offset: usize,
    pub len: usize,
}

/// A typed value representation for TSX attributes produced by the parser.
#[derive(Debug, Clone)]
pub enum TsTypeValue {
    Number(f64),
    String(SmolStr),
    Boolean(bool),
    Identifier(MtsmBindHandle),
    Null,
    Undefined,
}

/// Helper that binds a typed value to a slot or parameter name.
#[derive(Debug, Clone)]
pub struct TypeValueBinder {
    pub name: SmolStr,
    pub ttype: TsTypeDef,
    pub value: TsTypeValue,
}

#[derive(Debug)]
pub struct TsxElement {
    pub id: u32,
    pub kind: TsxKind,
    pub attributes: TsxAttributes,
    pub children: Option<TsxFragment>,
}

#[derive(Debug)]
pub struct TsxAttributes {
    pub attributes: DashMap<SmolStr, TsTypeValue>,
}

#[derive(Debug)]
pub struct TsxFragment {
    pub elements: Vec<TsxElement>,
}
