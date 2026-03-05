//! TSX → Asm bytecode compiler.
//!
//! Parses TSX using OXC and emits `Asm` tokens (pixel-coordinate draw commands)
//! for the MatterStream RPN VM. Supports:
//! - Primitive components: Box, Slab, Circle, Text, Line, Path, VStack
//! - Arrow-function composite components with prop substitution
//! - Compile-time arithmetic in numeric expressions (e.g. `y + 8`)

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::SourceType;

use matterstream_vm_asm::{Asm, AsmOutput};

// ── Intermediate representation ──────────────────────────────────────────

/// A prop value — either a string literal or a resolved numeric value.
#[derive(Debug, Clone)]
pub enum PropValue {
    Str(String),
    Num(i64),
}

/// A flattened JSX element with resolved props.
#[derive(Debug, Clone)]
struct JsxNode {
    tag: String,
    props: Vec<(String, PropValue)>,
    children: Vec<JsxNode>,
}

/// An arrow-function component definition.
#[derive(Debug, Clone)]
struct ComponentDef {
    params: Vec<String>,
    body: Vec<JsxNode>,
}

// ── OXC → JsxNode lowering ──────────────────────────────────────────────

struct AsmCompiler {
    components: HashMap<String, ComponentDef>,
}

impl AsmCompiler {
    fn new() -> Self {
        Self {
            components: HashMap::new(),
        }
    }

    /// First pass: collect arrow-function component definitions from
    /// `const Name = ({ params }) => ( JSX );` variable declarations.
    fn collect_components<'a>(&mut self, program: &Program<'a>) -> Result<(), String> {
        for stmt in &program.body {
            if let Statement::VariableDeclaration(decl) = stmt {
                for declarator in &decl.declarations {
                    self.try_collect_component(declarator)?;
                }
            }
        }
        Ok(())
    }

    fn try_collect_component<'a>(
        &mut self,
        declarator: &VariableDeclarator<'a>,
    ) -> Result<(), String> {
        // Name must be an identifier starting with uppercase
        let name = match &declarator.id.kind {
            BindingPatternKind::BindingIdentifier(id) => id.name.to_string(),
            _ => return Ok(()),
        };
        if name.is_empty() || !name.chars().next().unwrap().is_uppercase() {
            return Ok(());
        }

        // Init must be an arrow function expression
        let init = match &declarator.init {
            Some(init) => init,
            None => return Ok(()),
        };
        let arrow = match init {
            Expression::ArrowFunctionExpression(arrow) => arrow,
            _ => return Ok(()),
        };

        // Extract destructured params
        let params = self.extract_arrow_params(arrow);

        // Extract JSX body from the arrow function
        let body_nodes = self.extract_arrow_body(arrow)?;

        self.components.insert(name, ComponentDef { params, body: body_nodes });
        Ok(())
    }

    fn extract_arrow_params<'a>(&self, arrow: &ArrowFunctionExpression<'a>) -> Vec<String> {
        let mut params = Vec::new();
        for param in &arrow.params.items {
            match &param.pattern.kind {
                // ({ a, b, c }) destructured
                BindingPatternKind::ObjectPattern(obj) => {
                    for prop in &obj.properties {
                        if let PropertyKey::StaticIdentifier(id) = &prop.key {
                            params.push(id.name.to_string());
                        }
                    }
                }
                // (props) single identifier
                BindingPatternKind::BindingIdentifier(id) => {
                    params.push(id.name.to_string());
                }
                _ => {}
            }
        }
        params
    }

    fn extract_arrow_body<'a>(
        &self,
        arrow: &ArrowFunctionExpression<'a>,
    ) -> Result<Vec<JsxNode>, String> {
        {
            let body = &*arrow.body;
            let statements = &body.statements;
            {
                // Look for expression statement or return with JSX
                for stmt in statements {
                    match stmt {
                        Statement::ExpressionStatement(expr_stmt) => {
                            return self.lower_expression(&expr_stmt.expression, &HashMap::new());
                        }
                        Statement::ReturnStatement(ret) => {
                            if let Some(arg) = &ret.argument {
                                return self.lower_expression(arg, &HashMap::new());
                            }
                        }
                        _ => {}
                    }
                }
                // Expression body (parenthesized): the body itself might be interpreted
                // as a single expression statement
                Ok(Vec::new())
            }
        }
    }

    /// Second pass: lower top-level JSX expressions into JsxNodes.
    fn lower_program<'a>(&self, program: &Program<'a>) -> Result<Vec<JsxNode>, String> {
        let empty_ctx = HashMap::new();
        let mut nodes = Vec::new();
        for stmt in &program.body {
            match stmt {
                Statement::ExpressionStatement(expr_stmt) => {
                    let lowered = self.lower_expression(&expr_stmt.expression, &empty_ctx)?;
                    nodes.extend(lowered);
                }
                // Skip variable declarations (already processed in pass 1)
                Statement::VariableDeclaration(_) => {}
                _ => {}
            }
        }
        Ok(nodes)
    }

    /// Lower an expression to JsxNode(s).
    fn lower_expression<'a>(
        &self,
        expr: &Expression<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<JsxNode>, String> {
        match expr {
            Expression::JSXElement(el) => self.lower_jsx_element(el, ctx),
            Expression::JSXFragment(frag) => self.lower_jsx_fragment(frag, ctx),
            Expression::ParenthesizedExpression(paren) => {
                self.lower_expression(&paren.expression, ctx)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn lower_jsx_element<'a>(
        &self,
        element: &JSXElement<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<JsxNode>, String> {
        let tag = match &element.opening_element.name {
            JSXElementName::Identifier(id) => id.name.to_string(),
            _ => return Ok(Vec::new()),
        };

        // Extract props
        let props = self.extract_props(&element.opening_element.attributes, ctx)?;

        // Check if this is a composite component
        if let Some(comp) = self.components.get(&tag) {
            return self.expand_component(comp, &props, ctx);
        }

        // Lower children
        let children = self.lower_jsx_children(&element.children, ctx)?;

        Ok(vec![JsxNode { tag, props, children }])
    }

    fn lower_jsx_fragment<'a>(
        &self,
        fragment: &JSXFragment<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<JsxNode>, String> {
        self.lower_jsx_children(&fragment.children, ctx)
    }

    fn lower_jsx_children<'a>(
        &self,
        children: &[JSXChild<'a>],
        ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<JsxNode>, String> {
        let mut nodes = Vec::new();
        for child in children {
            match child {
                JSXChild::Element(el) => {
                    nodes.extend(self.lower_jsx_element(el, ctx)?);
                }
                JSXChild::Fragment(frag) => {
                    nodes.extend(self.lower_jsx_fragment(frag, ctx)?);
                }
                JSXChild::ExpressionContainer(container) => {
                    if let Some(expr) = container.expression.as_expression() {
                        nodes.extend(self.lower_expression(expr, ctx)?);
                    }
                }
                _ => {} // text, spread
            }
        }
        Ok(nodes)
    }

    /// Expand a composite component by substituting props into its body.
    fn expand_component(
        &self,
        comp: &ComponentDef,
        call_props: &[(String, PropValue)],
        parent_ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<JsxNode>, String> {
        let mut sub_ctx = parent_ctx.clone();
        for (key, val) in call_props {
            sub_ctx.insert(key.clone(), val.clone());
        }

        let mut result = Vec::new();
        for node in &comp.body {
            result.extend(self.substitute_node(node, &sub_ctx)?);
        }
        Ok(result)
    }

    /// Recursively substitute prop values in a pre-lowered node.
    fn substitute_node(
        &self,
        node: &JsxNode,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<JsxNode>, String> {
        // If this node is itself a composite component, expand it
        if let Some(comp) = self.components.get(&node.tag) {
            let resolved_props: Vec<(String, PropValue)> = node
                .props
                .iter()
                .map(|(k, v)| (k.clone(), resolve_prop(v, ctx)))
                .collect();
            return self.expand_component(comp, &resolved_props, ctx);
        }

        let new_props: Vec<(String, PropValue)> = node
            .props
            .iter()
            .map(|(k, v)| (k.clone(), resolve_prop(v, ctx)))
            .collect();

        let mut new_children = Vec::new();
        for child in &node.children {
            new_children.extend(self.substitute_node(child, ctx)?);
        }

        Ok(vec![JsxNode {
            tag: node.tag.clone(),
            props: new_props,
            children: new_children,
        }])
    }

    /// Extract props from JSX attributes, evaluating expressions against the context.
    fn extract_props<'a>(
        &self,
        attrs: &[JSXAttributeItem<'a>],
        ctx: &HashMap<String, PropValue>,
    ) -> Result<Vec<(String, PropValue)>, String> {
        let mut props = Vec::new();
        for item in attrs {
            if let JSXAttributeItem::Attribute(attr) = item {
                if let JSXAttributeName::Identifier(name) = &attr.name {
                    let key = name.name.to_string();
                    let val = self.extract_attr_value(attr, ctx)?;
                    props.push((key, val));
                }
            }
        }
        Ok(props)
    }

    fn extract_attr_value<'a>(
        &self,
        attr: &JSXAttribute<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<PropValue, String> {
        match &attr.value {
            Some(JSXAttributeValue::StringLiteral(s)) => Ok(PropValue::Str(s.value.to_string())),
            Some(JSXAttributeValue::ExpressionContainer(container)) => {
                self.eval_jsx_expression(&container.expression, ctx)
            }
            None => Ok(PropValue::Str(String::new())),
            _ => Ok(PropValue::Str(String::new())),
        }
    }

    fn eval_jsx_expression<'a>(
        &self,
        expr: &JSXExpression<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<PropValue, String> {
        match expr {
            JSXExpression::EmptyExpression(_) => Ok(PropValue::Str(String::new())),
            JSXExpression::BooleanLiteral(b) => Ok(PropValue::Num(b.value as i64)),
            JSXExpression::NullLiteral(_) => Ok(PropValue::Str(String::new())),
            JSXExpression::NumericLiteral(n) => Ok(PropValue::Num(n.value as i64)),
            JSXExpression::StringLiteral(s) => Ok(PropValue::Str(s.value.to_string())),
            JSXExpression::Identifier(id) => {
                let name = id.name.to_string();
                if let Some(val) = ctx.get(&name) {
                    Ok(val.clone())
                } else {
                    // Unresolved identifier — keep as string placeholder
                    Ok(PropValue::Str(name))
                }
            }
            JSXExpression::BinaryExpression(bin) => self.eval_binary(bin, ctx),
            JSXExpression::UnaryExpression(unary) => {
                if unary.operator.as_str() == "-" {
                    let val = self.eval_any_expression(&unary.argument, ctx)?;
                    match val {
                        PropValue::Num(n) => Ok(PropValue::Num(-n)),
                        other => Ok(other),
                    }
                } else {
                    Ok(PropValue::Num(0))
                }
            }
            // Fallback: try eval as general expression
            _ => {
                if let Some(expr) = expr.as_expression() {
                    self.eval_any_expression(expr, ctx)
                } else {
                    Ok(PropValue::Num(0))
                }
            }
        }
    }

    fn eval_any_expression<'a>(
        &self,
        expr: &Expression<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<PropValue, String> {
        match expr {
            Expression::NumericLiteral(n) => Ok(PropValue::Num(n.value as i64)),
            Expression::StringLiteral(s) => Ok(PropValue::Str(s.value.to_string())),
            Expression::Identifier(id) => {
                let name = id.name.to_string();
                if let Some(val) = ctx.get(&name) {
                    Ok(val.clone())
                } else {
                    Ok(PropValue::Str(name))
                }
            }
            Expression::BinaryExpression(bin) => self.eval_binary(bin, ctx),
            Expression::ParenthesizedExpression(paren) => {
                self.eval_any_expression(&paren.expression, ctx)
            }
            Expression::UnaryExpression(unary) => {
                if unary.operator.as_str() == "-" {
                    let val = self.eval_any_expression(&unary.argument, ctx)?;
                    match val {
                        PropValue::Num(n) => Ok(PropValue::Num(-n)),
                        other => Ok(other),
                    }
                } else {
                    Ok(PropValue::Num(0))
                }
            }
            _ => Ok(PropValue::Num(0)),
        }
    }

    fn eval_binary<'a>(
        &self,
        bin: &BinaryExpression<'a>,
        ctx: &HashMap<String, PropValue>,
    ) -> Result<PropValue, String> {
        let left = self.eval_any_expression(&bin.left, ctx)?;
        let right = self.eval_any_expression(&bin.right, ctx)?;
        match (left, right) {
            (PropValue::Num(l), PropValue::Num(r)) => {
                let result = match bin.operator.as_str() {
                    "+" => l + r,
                    "-" => l - r,
                    "*" => l * r,
                    "/" => if r != 0 { l / r } else { 0 },
                    "%" => if r != 0 { l % r } else { 0 },
                    _ => 0,
                };
                Ok(PropValue::Num(result))
            }
            _ => Ok(PropValue::Num(0)),
        }
    }
}

fn resolve_prop(val: &PropValue, ctx: &HashMap<String, PropValue>) -> PropValue {
    match val {
        PropValue::Str(s) => {
            if let Some(resolved) = ctx.get(s) {
                resolved.clone()
            } else {
                PropValue::Str(s.clone())
            }
        }
        PropValue::Num(n) => PropValue::Num(*n),
    }
}

// ── SVG path parsing & Bezier flattening ────────────────────────────────

#[derive(Debug, Clone)]
enum PathCmd {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    CubicTo(f64, f64, f64, f64, f64, f64),
    Close,
}

/// Parse an SVG `d` attribute string into path commands.
/// Supports: M/m, L/l, H/h, V/v, C/c, Z/z (absolute and relative).
fn parse_svg_path(d: &str) -> Vec<PathCmd> {
    let mut cmds = Vec::new();
    let mut cur_x: f64 = 0.0;
    let mut cur_y: f64 = 0.0;
    let mut start_x: f64 = 0.0;
    let mut start_y: f64 = 0.0;

    // Tokenize: split on command letters, keeping them as delimiters
    let mut tokens: Vec<String> = Vec::new();
    let mut buf = String::new();
    for ch in d.chars() {
        if "MmLlHhVvCcZz".contains(ch) {
            if !buf.is_empty() {
                tokens.push(buf.clone());
                buf.clear();
            }
            tokens.push(ch.to_string());
        } else {
            buf.push(ch);
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }

    fn parse_nums(s: &str) -> Vec<f64> {
        // Handle comma and space separated numbers, including negative signs
        let mut nums = Vec::new();
        let mut buf = String::new();
        for ch in s.chars() {
            if ch == ',' || ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                if !buf.is_empty() {
                    if let Ok(n) = buf.parse::<f64>() {
                        nums.push(n);
                    }
                    buf.clear();
                }
            } else if ch == '-' && !buf.is_empty() {
                // Negative sign starts a new number
                if let Ok(n) = buf.parse::<f64>() {
                    nums.push(n);
                }
                buf.clear();
                buf.push(ch);
            } else {
                buf.push(ch);
            }
        }
        if !buf.is_empty() {
            if let Ok(n) = buf.parse::<f64>() {
                nums.push(n);
            }
        }
        nums
    }

    let mut i = 0;
    while i < tokens.len() {
        let cmd_ch = tokens[i].chars().next().unwrap_or(' ');
        let nums = if i + 1 < tokens.len() && !"MmLlHhVvCcZz".contains(tokens[i + 1].chars().next().unwrap_or(' ')) {
            i += 1;
            parse_nums(&tokens[i])
        } else {
            Vec::new()
        };
        i += 1;

        match cmd_ch {
            'M' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    cur_x = nums[j];
                    cur_y = nums[j + 1];
                    if j == 0 {
                        start_x = cur_x;
                        start_y = cur_y;
                        cmds.push(PathCmd::MoveTo(cur_x, cur_y));
                    } else {
                        cmds.push(PathCmd::LineTo(cur_x, cur_y));
                    }
                    j += 2;
                }
            }
            'm' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    cur_x += nums[j];
                    cur_y += nums[j + 1];
                    if j == 0 {
                        start_x = cur_x;
                        start_y = cur_y;
                        cmds.push(PathCmd::MoveTo(cur_x, cur_y));
                    } else {
                        cmds.push(PathCmd::LineTo(cur_x, cur_y));
                    }
                    j += 2;
                }
            }
            'L' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    cur_x = nums[j];
                    cur_y = nums[j + 1];
                    cmds.push(PathCmd::LineTo(cur_x, cur_y));
                    j += 2;
                }
            }
            'l' => {
                let mut j = 0;
                while j + 1 < nums.len() {
                    cur_x += nums[j];
                    cur_y += nums[j + 1];
                    cmds.push(PathCmd::LineTo(cur_x, cur_y));
                    j += 2;
                }
            }
            'H' => {
                for n in &nums {
                    cur_x = *n;
                    cmds.push(PathCmd::LineTo(cur_x, cur_y));
                }
            }
            'h' => {
                for n in &nums {
                    cur_x += *n;
                    cmds.push(PathCmd::LineTo(cur_x, cur_y));
                }
            }
            'V' => {
                for n in &nums {
                    cur_y = *n;
                    cmds.push(PathCmd::LineTo(cur_x, cur_y));
                }
            }
            'v' => {
                for n in &nums {
                    cur_y += *n;
                    cmds.push(PathCmd::LineTo(cur_x, cur_y));
                }
            }
            'C' => {
                let mut j = 0;
                while j + 5 < nums.len() {
                    let (x1, y1) = (nums[j], nums[j + 1]);
                    let (x2, y2) = (nums[j + 2], nums[j + 3]);
                    let (x3, y3) = (nums[j + 4], nums[j + 5]);
                    cmds.push(PathCmd::CubicTo(x1, y1, x2, y2, x3, y3));
                    cur_x = x3;
                    cur_y = y3;
                    j += 6;
                }
            }
            'c' => {
                let mut j = 0;
                while j + 5 < nums.len() {
                    let (x1, y1) = (cur_x + nums[j], cur_y + nums[j + 1]);
                    let (x2, y2) = (cur_x + nums[j + 2], cur_y + nums[j + 3]);
                    let (x3, y3) = (cur_x + nums[j + 4], cur_y + nums[j + 5]);
                    cmds.push(PathCmd::CubicTo(x1, y1, x2, y2, x3, y3));
                    cur_x = x3;
                    cur_y = y3;
                    j += 6;
                }
            }
            'Z' | 'z' => {
                cmds.push(PathCmd::Close);
                cur_x = start_x;
                cur_y = start_y;
            }
            _ => {}
        }
    }
    cmds
}

/// Flatten a cubic Bezier curve into line segments using recursive de Casteljau subdivision.
fn flatten_cubic(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    tolerance: f64,
    out: &mut Vec<(f64, f64)>,
) {
    // Check if curve is flat enough: max distance of control points from the line p0→p3
    let dx = p3.0 - p0.0;
    let dy = p3.1 - p0.1;
    let len_sq = dx * dx + dy * dy;

    let d1 = if len_sq > 0.0 {
        let t = ((p1.0 - p0.0) * dx + (p1.1 - p0.1) * dy) / len_sq;
        let proj_x = p0.0 + t * dx;
        let proj_y = p0.1 + t * dy;
        let ex = p1.0 - proj_x;
        let ey = p1.1 - proj_y;
        ex * ex + ey * ey
    } else {
        let ex = p1.0 - p0.0;
        let ey = p1.1 - p0.1;
        ex * ex + ey * ey
    };

    let d2 = if len_sq > 0.0 {
        let t = ((p2.0 - p0.0) * dx + (p2.1 - p0.1) * dy) / len_sq;
        let proj_x = p0.0 + t * dx;
        let proj_y = p0.1 + t * dy;
        let ex = p2.0 - proj_x;
        let ey = p2.1 - proj_y;
        ex * ex + ey * ey
    } else {
        let ex = p2.0 - p0.0;
        let ey = p2.1 - p0.1;
        ex * ex + ey * ey
    };

    let tol_sq = tolerance * tolerance;
    if d1 <= tol_sq && d2 <= tol_sq {
        out.push(p3);
        return;
    }

    // Subdivide at t=0.5
    let mid01 = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
    let mid12 = ((p1.0 + p2.0) * 0.5, (p1.1 + p2.1) * 0.5);
    let mid23 = ((p2.0 + p3.0) * 0.5, (p2.1 + p3.1) * 0.5);
    let mid012 = ((mid01.0 + mid12.0) * 0.5, (mid01.1 + mid12.1) * 0.5);
    let mid123 = ((mid12.0 + mid23.0) * 0.5, (mid12.1 + mid23.1) * 0.5);
    let mid0123 = ((mid012.0 + mid123.0) * 0.5, (mid012.1 + mid123.1) * 0.5);

    flatten_cubic(p0, mid01, mid012, mid0123, tolerance, out);
    flatten_cubic(mid0123, mid123, mid23, p3, tolerance, out);
}

// ── JsxNode → Asm emission ──────────────────────────────────────────────

fn parse_color(color: &str) -> (u8, u8, u8, u8) {
    let hex = color.trim_start_matches('#');
    let r = u8::from_str_radix(hex.get(0..2).unwrap_or("00"), 16).unwrap_or(0);
    let g = u8::from_str_radix(hex.get(2..4).unwrap_or("00"), 16).unwrap_or(0);
    let b = u8::from_str_radix(hex.get(4..6).unwrap_or("00"), 16).unwrap_or(0);
    let a = if hex.len() >= 8 {
        u8::from_str_radix(&hex[6..8], 16).unwrap_or(0xFF)
    } else {
        0xFF
    };
    (r, g, b, a)
}

fn get_str_prop(props: &[(String, PropValue)], name: &str) -> Option<String> {
    props.iter().find_map(|(k, v)| {
        if k == name {
            match v {
                PropValue::Str(s) => Some(s.clone()),
                PropValue::Num(n) => Some(n.to_string()),
            }
        } else {
            None
        }
    })
}

fn get_num_prop(props: &[(String, PropValue)], name: &str) -> Option<i64> {
    props.iter().find_map(|(k, v)| {
        if k == name {
            match v {
                PropValue::Num(n) => Some(*n),
                PropValue::Str(s) => s.parse::<i64>().ok(),
            }
        } else {
            None
        }
    })
}

fn emit_nodes(asm: &mut Asm, nodes: &[JsxNode]) {
    for node in nodes {
        emit_node(asm, node);
    }
}

fn emit_node(asm: &mut Asm, node: &JsxNode) {
    match node.tag.as_str() {
        "Box" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let w = get_num_prop(&node.props, "w").unwrap_or(100) as u32;
            let h = get_num_prop(&node.props, "h").unwrap_or(100) as u32;
            if let Some(color) = get_str_prop(&node.props, "color") {
                let (r, g, b, a) = parse_color(&color);
                asm.set_color(r, g, b, a);
            }
            asm.draw_box(x, y, w, h);
        }
        "Slab" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let w = get_num_prop(&node.props, "w").unwrap_or(100) as u32;
            let h = get_num_prop(&node.props, "h").unwrap_or(100) as u32;
            let radius = get_num_prop(&node.props, "radius").unwrap_or(4) as u32;
            if let Some(color) = get_str_prop(&node.props, "color") {
                let (r, g, b, a) = parse_color(&color);
                asm.set_color(r, g, b, a);
            }
            asm.draw_slab(x, y, w, h, radius);
        }
        "Circle" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let r = get_num_prop(&node.props, "r").unwrap_or(10) as u32;
            if let Some(color) = get_str_prop(&node.props, "color") {
                let (cr, cg, cb, ca) = parse_color(&color);
                asm.set_color(cr, cg, cb, ca);
            }
            asm.draw_circle(x, y, r);
        }
        "Text" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let size = get_num_prop(&node.props, "size").unwrap_or(14) as u32;
            let label = get_str_prop(&node.props, "label").unwrap_or_default();
            if let Some(color) = get_str_prop(&node.props, "color") {
                let (r, g, b, a) = parse_color(&color);
                asm.set_color(r, g, b, a);
            }
            let str_id = asm.def_string(&label);
            asm.draw_text_str(x, y, size, str_id);
        }
        "Line" => {
            let x1 = get_num_prop(&node.props, "x1").unwrap_or(0) as i32;
            let y1 = get_num_prop(&node.props, "y1").unwrap_or(0) as i32;
            let x2 = get_num_prop(&node.props, "x2").unwrap_or(0) as i32;
            let y2 = get_num_prop(&node.props, "y2").unwrap_or(0) as i32;
            if let Some(color) = get_str_prop(&node.props, "color") {
                let (r, g, b, a) = parse_color(&color);
                asm.set_color(r, g, b, a);
            }
            asm.draw_line(x1, y1, x2, y2);
        }
        "Path" => {
            let ox = get_num_prop(&node.props, "x").unwrap_or(0) as f64;
            let oy = get_num_prop(&node.props, "y").unwrap_or(0) as f64;
            let _stroke = get_num_prop(&node.props, "stroke").unwrap_or(2);
            if let Some(color) = get_str_prop(&node.props, "color") {
                let (r, g, b, a) = parse_color(&color);
                asm.set_color(r, g, b, a);
            }
            if let Some(d) = get_str_prop(&node.props, "d") {
                let cmds = parse_svg_path(&d);
                let mut cur = (0.0_f64, 0.0_f64);
                let mut start = (0.0_f64, 0.0_f64);
                for cmd in &cmds {
                    match cmd {
                        PathCmd::MoveTo(x, y) => {
                            cur = (*x, *y);
                            start = cur;
                        }
                        PathCmd::LineTo(x, y) => {
                            asm.draw_line(
                                (cur.0 + ox) as i32,
                                (cur.1 + oy) as i32,
                                (*x + ox) as i32,
                                (*y + oy) as i32,
                            );
                            cur = (*x, *y);
                        }
                        PathCmd::CubicTo(x1, y1, x2, y2, x3, y3) => {
                            let mut pts = Vec::new();
                            flatten_cubic(
                                cur,
                                (*x1, *y1),
                                (*x2, *y2),
                                (*x3, *y3),
                                1.0,
                                &mut pts,
                            );
                            for pt in &pts {
                                asm.draw_line(
                                    (cur.0 + ox) as i32,
                                    (cur.1 + oy) as i32,
                                    (pt.0 + ox) as i32,
                                    (pt.1 + oy) as i32,
                                );
                                cur = *pt;
                            }
                        }
                        PathCmd::Close => {
                            if (cur.0 - start.0).abs() > 0.5 || (cur.1 - start.1).abs() > 0.5 {
                                asm.draw_line(
                                    (cur.0 + ox) as i32,
                                    (cur.1 + oy) as i32,
                                    (start.0 + ox) as i32,
                                    (start.1 + oy) as i32,
                                );
                            }
                            cur = start;
                        }
                    }
                }
            }
        }
        "VStack" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            asm.ui_push_state();
            asm.ui_set_offset(x, y);
            emit_nodes(asm, &node.children);
            asm.ui_pop_state();
            return; // children already emitted
        }
        _ => {
            // Unknown tag — emit children only
        }
    }

    // Emit children (except VStack which returns early)
    emit_nodes(asm, &node.children);
}

// ── Public API ───────────────────────────────────────────────────────────

/// Compile TSX source into Asm bytecode output.
///
/// Two-pass compilation:
/// 1. Collect arrow-function component definitions (`const Name = ({...}) => ...`)
/// 2. Lower top-level JSX expressions, expanding components inline, then emit Asm
pub fn compile_to_asm(tsx_source: &str) -> Result<AsmOutput, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path("card.tsx")
        .map_err(|e| format!("SourceType error: {:?}", e))?;
    let ret = Parser::new(&allocator, tsx_source, source_type).parse();

    if !ret.errors.is_empty() {
        let msgs: Vec<String> = ret.errors.into_iter().map(|e| format!("{:?}", e)).collect();
        return Err(msgs.join("\n"));
    }

    let mut compiler = AsmCompiler::new();

    // Pass 1: collect component definitions
    compiler.collect_components(&ret.program)?;

    // Pass 2: lower JSX to flat node tree (expanding components)
    let nodes = compiler.lower_program(&ret.program)?;

    // Pass 3: emit Asm bytecode
    let mut asm = Asm::new();
    emit_nodes(&mut asm, &nodes);
    asm.halt();

    asm.finish().map_err(|e| format!("asm error: {:?}", e))
}
