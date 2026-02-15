use dashmap::DashMap;

#[derive(Debug)]
pub enum TsxKind {
    Div,
    Span,
    Custom(String),
}

/// Simple type definitions for TSX attribute typing.
#[derive(Debug, Clone)]
pub enum TsTypeDef {
    Number,
    String,
    Boolean,
    Any,
}

/// A typed value representation for TSX attributes produced by the parser.
#[derive(Debug, Clone)]
pub enum TsTypeValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Identifier(String),
    Null,
    Undefined,
}

/// Helper that binds a typed value to a slot or parameter name.
#[derive(Debug, Clone)]
pub struct TypeValueBinder {
    pub name: String,
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
    pub attributes: DashMap<String, TsTypeValue>,
}

#[derive(Debug)]
pub struct TsxFragment {
    pub elements: Vec<TsxElement>,
}
