//! matterstream-parser
//!
//! This crate is responsible for parsing UI definitions, typically from TSX source,
//! into a structured intermediate representation known as `Parsed`.
//! It acts as the initial stage in the MatterStream UI processing pipeline,
//! converting raw source code into an Abstract Syntax Tree (AST) composed of
//! MatterStream-specific types (`TsxFragment`, `MtsmObject`, etc.).

use dashmap::DashMap;
use matterstream_core::{MtsmObject, MtsmVariant, TsxFragment, MtsmTsxFunctionalComponent, TsxElementContext, TsxAttributes, TsxElement, TsxKind, TsTypeValue};
use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, JSXElement as OxcJSXElement, JSXFragment as OxcJSXFragment, JSXAttribute as OxcJSXAttribute, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression, IdentifierReference, Statement, ModuleDeclaration, ImportDeclaration, ImportDeclarationSpecifier, JSXChild, Expression, ExpressionStatement, JSXElementName};
use oxc_span::SourceType;
use oxc_parser::Parser as OxcParser; // Alias to avoid conflict with our Parser
use std::collections::HashMap; // For imports

/// Represents the raw parsing result from `oxc_parser`.
///
/// This struct holds the Oxc AST `Program` and its associated `Allocator`.
pub struct RawParsed<'a> {
    /// The allocator used for the Oxc AST.
    pub allocator: &'a Allocator,
    /// The root of the Oxc AST.
    pub program: Program<'a>,
}

/// A visitor that transforms an Oxc AST into MatterStream's `Tsx*` and `Mtsm*` types.
pub struct MatterStreamToParsedVisitor<'a> {
    /// The allocator for Oxc AST nodes.
    allocator: &'a Allocator,
    /// Counter for generating unique IDs for TsxElements.
    next_id: u32,
    /// Collected Mtsm objects, such as import bindings.
    mtsm_data: MtsmObject,
    /// Store import declarations for later resolution.
    imports: HashMap<String, String>, // Local name -> Module specifier
    /// Binder to track identifier bindings discovered while parsing.
    binder: matterstream_core::Binder,
}

impl<'a> MatterStreamToParsedVisitor<'a> {
    /// Creates a new `MatterStreamToParsedVisitor`.
    pub fn new(allocator: &'a Allocator) -> Self {
        Self {
            allocator,
            next_id: 0,
            mtsm_data: MtsmObject { data: DashMap::new() },
            imports: HashMap::new(),
            binder: matterstream_core::Binder::new(),
        }
    }

    /// Transforms an Oxc `Program` into a MatterStream `Parsed` object.
    pub fn transform_program(&mut self, program: &Program<'a>) -> Result<Parsed, String> {
        let mut root_elements = Vec::new();

        for stmt in &program.body {
            if let Some(decl) = stmt.as_module_declaration() {
                if let ModuleDeclaration::ImportDeclaration(ref import_decl) = *decl { // Fixed: dereference decl and match by reference
                    // Handle imports
                    // Handle imports
                    if let Some(specifiers) = &import_decl.specifiers {
                        for specifier in specifiers.iter() {
                            match specifier {
                                ImportDeclarationSpecifier::ImportSpecifier(imp_spec) => {
                                    // Example: `import { Slab } from '@mtsm/ui/core';`
                                    let local = imp_spec.local.name.to_string();
                                    self.imports.insert(local.clone(), import_decl.source.value.to_string());
                                    // Register as late-bound identifier in binder (imports are resolved later)
                                    let _ = self.binder.insert_latebound(&local, Some(matterstream_core::TsTypeDef::Any), None);
                                }
                                _ => {} // Ignore other specifier types
                            }
                        }
                    }
                }
            } else if let Statement::ExpressionStatement(expr_stmt) = stmt {
                if let Expression::JSXElement(jsx_element) = &expr_stmt.expression {
                    root_elements.push(self.transform_jsx_element(jsx_element)?);
                } else if let Expression::JSXFragment(jsx_fragment) = &expr_stmt.expression {
                    let fragment = self.transform_jsx_fragment(jsx_fragment)?;
                    root_elements.extend(fragment.elements);
                }
            }
        }
        
        let root_fragment = TsxFragment { elements: root_elements };

        Ok(Parsed {
            root_fragment,
            mtsm_data: std::mem::take(&mut self.mtsm_data),
        })
    }

    /// Transforms an Oxc `JSXElement` into a MatterStream `TsxElement`.
    fn transform_jsx_element(&mut self, oxc_jsx_element: &OxcJSXElement<'a>) -> Result<TsxElement, String> {
        self.next_id += 1;
        let id = self.next_id;

        let kind = if let JSXElementName::Identifier(ident) = &oxc_jsx_element.opening_element.name { // Fixed: JSXElementName
            // Check if it's an imported component
            if let Some(module_specifier) = self.imports.get(&ident.name.to_string()) {
                // Here, we would store information about the imported component for later processing.
                // For now, let's treat it as a custom component for the TsxKind
                dbg!("Found imported component: {} from {}", &ident.name, module_specifier);
                TsxKind::Custom(ident.name.to_string())
            } else {
                match ident.name.as_str() {
                    "div" => TsxKind::Div,
                    "span" => TsxKind::Span,
                    _ => TsxKind::Custom(ident.name.to_string()),
                }
            }
        } else {
            // Handle other JSX element names (e.g., MemberExpression, JSXNamespacedName) if needed
            TsxKind::Custom("Unknown".to_string()) // Placeholder
        };

        // Extract attributes
        let attributes = self.transform_jsx_attributes(&oxc_jsx_element.opening_element.attributes)?;


        let mut children_elements = Vec::new();
        for child in &oxc_jsx_element.children {
            match child {
                oxc_ast::ast::JSXChild::Element(elem) => children_elements.push(self.transform_jsx_element(elem)?),
                oxc_ast::ast::JSXChild::Fragment(frag) => children_elements.extend(self.transform_jsx_fragment(frag)?.elements),
                // Handle other JSXChild types (e.g., ExpressionContainer, Text) if needed
                _ => eprintln!("Warning: Unhandled JSXChild type in TsxElement transformation: {:?}", child),
            }
        }
        let children = if children_elements.is_empty() { None } else { Some(TsxFragment { elements: children_elements }) };

        Ok(TsxElement {
            id,
            kind,
            attributes, // Add attributes here
            children,
        })
    }

    /// Transforms an Oxc `JSXFragment` into a MatterStream `TsxFragment`.
    fn transform_jsx_fragment(&mut self, oxc_jsx_fragment: &OxcJSXFragment<'a>) -> Result<TsxFragment, String> {
        let mut elements = Vec::new();
        for child in &oxc_jsx_fragment.children {
            match child {
                oxc_ast::ast::JSXChild::Element(elem) => elements.push(self.transform_jsx_element(elem)?),
                oxc_ast::ast::JSXChild::Fragment(frag) => elements.extend(self.transform_jsx_fragment(frag)?.elements),
                _ => eprintln!("Warning: Unhandled JSXChild type in TsxFragment transformation: {:?}", child),
            }
        }
        Ok(TsxFragment { elements })
    }

    // This method is not directly used for TsxAttributes in MatterStreamToParsedVisitor,
    // as TsxAttributes is built directly within transform_jsx_element context.
    // However, keeping it for conceptual clarity if needed elsewhere.
    fn transform_jsx_attributes(&mut self, oxc_jsx_attributes: &[JSXAttributeItem<'a>]) -> Result<TsxAttributes, String> {
        use matterstream_core::TsTypeValue;
use smol_str::SmolStr;
        let attributes_map: DashMap<SmolStr, TsTypeValue> = DashMap::new();
        for item in oxc_jsx_attributes {
            if let JSXAttributeItem::Attribute(attr) = item {
                if let JSXAttributeName::Identifier(name) = &attr.name {
                    let key = name.name.to_string();
                    let value = if let Some(attr_value) = &attr.value {
                        match attr_value {
                            JSXAttributeValue::StringLiteral(lit) => TsTypeValue::String(lit.value.to_string().into()),
                            JSXAttributeValue::ExpressionContainer(expr_container) => {
                                match &expr_container.expression {
                                    JSXExpression::StringLiteral(lit) => TsTypeValue::String(lit.value.to_string().into()),
                                    JSXExpression::NumericLiteral(lit) => TsTypeValue::Number(lit.value as f64),
                                    JSXExpression::Identifier(ident) => {
                                        let name = ident.name.to_string();
                                        // Attempt to record source location if available (not all Identifier types expose spans uniformly)
                                        let loc = None; // Placeholder until span extraction is implemented
                                        // Register identifier in binder as late-bound if not already present
                                        if !self.binder.contains(&name) {
                                            let _ = self.binder.insert_latebound(&name, None, loc.clone());
                                        }
                                        TsTypeValue::Identifier(SmolStr::new(name))
                                    }
                                    _ => {
                                        eprintln!("Warning: Unhandled JSX expression type for attribute '{}'", key);
                                        TsTypeValue::Undefined
                                    }
                                }
                            },
                            _ => {
                                eprintln!("Warning: Unhandled JSX attribute value type for attribute '{}'", key);
                                TsTypeValue::Undefined
                            }
                        }
                    } else {
                        TsTypeValue::Boolean(true) // Boolean attribute (e.g., <Component isDisabled />)
                    };
                    attributes_map.insert(SmolStr::new(key), value);
                }
            }
        }
        Ok(TsxAttributes { attributes: attributes_map })
    }
}

/// Represents the parsed UI structure obtained from processing source code.
///
/// This object contains the root of the UI's Abstract Syntax Tree (AST)
/// as a `TsxFragment`, and any associated MatterStream (Mtsm) data or bindings.
pub struct Parsed {
    /// The root `TsxFragment` representing the UI's structural elements.
    pub root_fragment: TsxFragment,
    /// A collection of MatterStream objects, bindings, or other associated data.
    pub mtsm_data: MtsmObject, // To hold any associated Mtsm data/bindings
}

/// A placeholder functional component type used by tests.
pub struct DummyFunctionalComponent;

/// The main parser for MatterStream UI definitions.
///
/// This parser takes raw TSX-like source code and transforms it into a `Parsed` object,
/// which is a structured AST ready for further processing by the `matterstream-processor`.
pub struct Parser;

impl Parser {
    /// Parses an input string containing UI definition into a `Parsed` object.
    ///
    /// # Arguments
    ///
    /// * `input` - A string slice containing the UI definition source code (e.g., TSX).
    ///
    /// # Returns
    ///
    /// A `Result` which is:
    /// - `Ok(Parsed)` containing the structured AST if parsing is successful.
    /// - `Err(String)` containing an error message if parsing fails.
    pub fn parse(input: &str) -> Result<Parsed, String> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path("input.tsx").unwrap();
        let ret = OxcParser::new(&allocator, input, source_type).parse();

        if !ret.errors.is_empty() {
            let error_messages: Vec<String> = ret.errors
                .into_iter()
                .map(|e| format!("{:?}", e))
                .collect();
            return Err(error_messages.join("\n"));
        }

        let raw_parsed = RawParsed {
            allocator: &allocator,
            program: ret.program,
        };

        let mut visitor = MatterStreamToParsedVisitor::new(raw_parsed.allocator);
        visitor.transform_program(&raw_parsed.program)
    }
}
