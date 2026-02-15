use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_ast::{visit::{self, Visit}, ast::*};
use anyhow::{Result, anyhow}; // Add this

use matterstream_core::{Op, Primitive, OpsHeader, CompiledOps};

pub type CompilerResult<T> = Result<T>;
pub type CompilerError = anyhow::Error;

pub struct Compiler;

impl Compiler {
    pub fn compile(source_text: &str) -> CompilerResult<CompiledOps> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path("example.tsx").unwrap();
        let ret = Parser::new(&allocator, source_text, source_type).parse();

        if !ret.errors.is_empty() {
            let error_messages: Vec<String> = ret.errors
                .into_iter()
                .map(|e| format!("{:?}", e))
                .collect();
            return Err(anyhow!(error_messages.join("\n")));
        }

        let mut visitor = MatterStreamVisitor::new();
        visitor.visit_program(&ret.program);

        let header = OpsHeader::new(vec![], false);

        Ok(CompiledOps::new(header, visitor.ops))
    }
}

#[derive(Default)]
struct MatterStreamVisitor {
    ops: Vec<Op>,
}

impl MatterStreamVisitor {
    fn new() -> Self {
        Self::default()
    }

    fn parse_rgba_hex(hex: &str) -> Option<[f32; 4]> {
        if hex.len() != 9 || !hex.starts_with('#') {
            return None;
        }

        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;
        let a = u8::from_str_radix(&hex[7..9], 16).ok()?;

        Some([
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ])
    }
}

impl<'a> Visit<'a> for MatterStreamVisitor {
    fn visit_jsx_element(&mut self, element: &JSXElement<'a>) {
        if let JSXElementName::Identifier(ident) = &element.opening_element.name {
            if ident.name == "Slab" {
                let mut x_val = 0.0;
                let mut y_val = 0.0;
                let mut color_val: Option<[f32; 4]> = None;

                for attribute in &element.opening_element.attributes {
                    if let JSXAttributeItem::Attribute(attr) = attribute {
                        if let JSXAttributeName::Identifier(name) = &attr.name {
                            if name.name == "x" {
                                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                                    if let JSXExpression::NumericLiteral(num_literal) = &container.expression {
                                        x_val = num_literal.value as f32;
                                    } else if let JSXExpression::Identifier(ident) = &container.expression {
                                        eprintln!("Warning: 'x' attribute is an identifier '{}'. Defaulting to 0.0", ident.name);
                                    }
                                }
                            } else if name.name == "y" {
                                if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                                    if let JSXExpression::NumericLiteral(num_literal) = &container.expression {
                                        y_val = num_literal.value as f32;
                                    } else if let JSXExpression::Identifier(ident) = &container.expression {
                                        eprintln!("Warning: 'y' attribute is an identifier '{}'. Defaulting to 0.0", ident.name);
                                    }
                                }
                            } else if name.name == "color" {
                                let color_string_option = if let Some(JSXAttributeValue::StringLiteral(str_literal)) = &attr.value {
                                    Some(&str_literal.value)
                                } else if let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value {
                                    if let JSXExpression::StringLiteral(str_literal) = &container.expression {
                                        Some(&str_literal.value)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                if let Some(color_str) = color_string_option {
                                    if let Some(parsed_color) = Self::parse_rgba_hex(color_str) {
                                        color_val = Some(parsed_color);
                                    } else {
                                        eprintln!("Warning: 'color' attribute has invalid hex format '{}'. Defaulting to white.", color_str);
                                    }
                                } else {
                                    eprintln!("Warning: 'color' attribute is not a valid string literal or expression container. Defaulting to white.");
                                }
                            }
                        }
                    }
                }

                if let Some(color) = color_val {
                    self.ops.push(Op::SetColor(color));
                } else {
                    self.ops.push(Op::SetColor([1.0, 1.0, 1.0, 1.0]));
                }
                self.ops.push(Op::SetTrans([x_val, y_val, 0.0]));
                self.ops.push(Op::Draw { primitive: Primitive::Slab, position_rsi: 0 });
            }
        }

        visit::walk::walk_jsx_element(self, element);
    }
}
