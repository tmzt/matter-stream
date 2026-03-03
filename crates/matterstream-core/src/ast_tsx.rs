pub struct TsxElement {
    pub id: u32,
    pub kind: TsxKind,
    pub children: Option<TsxFragment>,
}

pub struct TsxAttributes {
    pub attributes: DashMap<String, String>,
}

pub struct TsxFragment {
    pub elements: Vec<TsxElement>,
}
