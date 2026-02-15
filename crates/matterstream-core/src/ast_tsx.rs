use dashmap::DashMap;

#[derive(Debug)]
pub enum TsxKind {
    Div,
    Span,
    Custom(String),
}

#[derive(Debug)]
pub struct TsxElement {
    pub id: u32,
    pub kind: TsxKind,
    pub children: Option<TsxFragment>,
}

#[derive(Debug)]
pub struct TsxAttributes {
    pub attributes: DashMap<String, String>,
}

#[derive(Debug)]
pub struct TsxFragment {
    pub elements: Vec<TsxElement>,
}
