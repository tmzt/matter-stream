use matterstream_parser::{Parser, Parsed, DummyFunctionalComponent};
use matterstream_core::{TsxFragment, MtsmObject};
use dashmap::DashMap;

#[test]
fn test_parser_returns_dummy_parsed_object() {
    let input = r#"<div />;"#;
    let parsed = Parser::parse(input).expect("Parser should not fail for dummy implementation");

    // Verify the root_fragment contains at least one element
    assert!(!parsed.root_fragment.elements.is_empty());

    // Parser now inserts a binder marker into mtsm_data; ensure it's present
    assert!(parsed.mtsm_data.data.contains_key("__binder"));

    // Ensure the first element is a div kind
    let first = &parsed.root_fragment.elements[0];
    match &first.kind {
        matterstream_core::TsxKind::Div => (),
        other => panic!("Expected first element to be Div, got: {:?}", other),
    }

    // Ensure attributes map is present and empty for a self-closing div
    assert!(first.attributes.attributes.is_empty());

    // Ensure there are no children
    assert!(first.children.is_none());

    // Ensure mtsm_data contains imports map and binder marker
    assert!(parsed.mtsm_data.data.contains_key("__binder"));
    // imports may be empty for this test
}
