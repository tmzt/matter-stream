use matterstream_parser::{Parser, Parsed, DummyFunctionalComponent};
use matterstream_core::{TsxFragment, MtsmObject};
use dashmap::DashMap;
use smol_str::SmolStr;

#[test]
fn test_parser_returns_dummy_parsed_object() {
    let input = r##"<div />;"##;
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

#[test]
fn test_parser_parses_attributes_and_values() {
    let input = r##"<div title="hello" count={42} visible />;"##;
    let parsed = Parser::parse(input).expect("Parser should parse attributes");
    let first = &parsed.root_fragment.elements[0];

    // title => String("hello")
    let title_key = SmolStr::new("title");
    let title_val = first.attributes.attributes.get(&title_key).expect("title attribute missing");
    match &*title_val {
        matterstream_core::TsTypeValue::String(s) => assert_eq!(s.as_str(), "hello"),
        other => panic!("Expected title to be String, got: {:?}", other),
    }

    // count => Number(42)
    let count_key = SmolStr::new("count");
    let count_val = first.attributes.attributes.get(&count_key).expect("count attribute missing");
    match &*count_val {
        matterstream_core::TsTypeValue::Number(n) => assert_eq!(*n as i64, 42),
        other => panic!("Expected count to be Number, got: {:?}", other),
    }

    // visible => Boolean(true)
    let vis_key = SmolStr::new("visible");
    let vis_val = first.attributes.attributes.get(&vis_key).expect("visible attribute missing");
    match &*vis_val {
        matterstream_core::TsTypeValue::Boolean(b) => assert!(*b),
        other => panic!("Expected visible to be Boolean, got: {:?}", other),
    }
}

#[test]
fn test_parser_parses_identifier_attribute_and_children() {
    let input = r##"<div ref={myRef}><span /></div>;"##;
    let parsed = Parser::parse(input).expect("Parser should parse identifier attribute and children");
    let first = &parsed.root_fragment.elements[0];

    // ref => Identifier(myRef)
    let ref_key = SmolStr::new("ref");
    let ref_val = first.attributes.attributes.get(&ref_key).expect("ref attribute missing");
    match &*ref_val {
        matterstream_core::TsTypeValue::Identifier(handle) => {
            // ensure binder assigned a handle and that lookups work
            assert!(handle.0 != 0);
        }
        other => panic!("Expected ref to be Identifier handle, got: {:?}", other),
    }

    // children -> span element
    let children = first.children.as_ref().expect("Expected children");
    assert_eq!(children.elements.len(), 1);
    match &children.elements[0].kind {
        matterstream_core::TsxKind::Span => (),
        other => panic!("Expected child to be Span, got: {:?}", other),
    }
}

#[test]
fn test_parser_handles_imports_and_custom_components() {
    let input = r##"import { Slab } from '@mtsm/ui/core';
<Slab />;"##;
    let parsed = Parser::parse(input).expect("Parser should parse import and custom component");
    let first = &parsed.root_fragment.elements[0];

    match &first.kind {
        matterstream_core::TsxKind::Custom(handle) => assert!(handle.0 != 0),
        other => panic!("Expected Custom Slab handle, got: {:?}", other),
    }

    // binder marker still present
    assert!(parsed.mtsm_data.data.contains_key("__binder"));
}

