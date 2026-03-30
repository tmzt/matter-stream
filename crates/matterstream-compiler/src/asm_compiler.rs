//! TSX → Asm bytecode compiler.
//!
//! Parses TSX using OXC and emits `Asm` tokens (pixel-coordinate draw commands)
//! for the MatterStream RPN VM. Supports:
//! - Primitive components: Box, Slab, Circle, Text, Line, Path, VStack
//! - Arrow-function composite components with prop substitution
//! - Compile-time arithmetic in numeric expressions (e.g. `y + 8`)
//! - VQL queries: Query, Bind, Field, Filter, Param
//! - SKLL skills: Skill, Step, LlmStep, Replaceable, Invoke

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::SourceType;

use matterstream_vm::ui_vm::{LlmUseCase, FOURCC_MTUI, FOURCC_VQL0, FOURCC_SKLL};
use matterstream_vm_asm::{Asm, AsmOutput, TkvTemplate};
use matterstream_vm_addressing::{TkvKey, TkvFixedEntry, TkvType, StrRefDisc};
use matterstream_vm_addressing::oid::Oid;

// ── Intermediate representation ──────────────────────────────────────────

/// Bank type constants for packed refs (u16 bank_type << 16 | u16 slot).
const BANK_INT: u16 = 1;
#[allow(dead_code)]
const BANK_SCALAR: u16 = 0;
#[allow(dead_code)]
const BANK_STRING: u16 = 5;

/// A prop value — string literal or resolved numeric value.
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
    _params: Vec<String>,
    body: Vec<JsxNode>,
}

// ── OXC → JsxNode lowering ──────────────────────────────────────────────

/// A useState binding: variable name → packed bank ref.
/// How a state binding is backed at runtime.
#[derive(Debug, Clone)]
enum StateBackend {
    /// Local IntBank slot — useState(value).
    IntBank { packed_ref: u32, initial_value: i64 },
    /// UserAtomicReadable — useMicState() etc.
    UserAtomic { slot: u32 },
}

#[derive(Debug, Clone)]
struct StateBinding {
    name: String,
    _setter_name: String,
    backend: StateBackend,
}

/// Maps import paths to component OIDs for cross-package resolution.
pub struct ImportMap {
    /// "@chitin/ui-kit" → { "StatusBar" → Oid, "InboxCard" → Oid, ... }
    pub packages: HashMap<String, HashMap<String, matterstream_vm_addressing::oid::Oid>>,
}

impl ImportMap {
    pub fn new() -> Self { Self { packages: HashMap::new() } }
}

struct AsmCompiler {
    components: HashMap<String, ComponentDef>,
    /// External components from import statements (name → OID).
    external_components: HashMap<String, matterstream_vm_addressing::oid::Oid>,
    /// useState bindings: name → packed ref.
    state_bindings: Vec<StateBinding>,
    /// Next free IntBank slot for useState(bool/int).
    next_int_slot: u16,
}

impl AsmCompiler {
    fn new() -> Self {
        Self {
            components: HashMap::new(),
            external_components: HashMap::new(),
            state_bindings: Vec::new(),
            next_int_slot: 0,
        }
    }

    /// Allocate an IntBank slot for useState. Returns packed ref.
    fn alloc_int_state(&mut self, name: &str, setter_name: &str, initial: i64) -> u32 {
        let slot = self.next_int_slot;
        self.next_int_slot += 1;
        let packed_ref = (BANK_INT as u32) << 16 | slot as u32;
        self.state_bindings.push(StateBinding {
            name: name.to_string(),
            _setter_name: setter_name.to_string(),
            backend: StateBackend::IntBank { packed_ref, initial_value: initial },
        });
        packed_ref
    }

    /// Allocate a UserAtomic-backed hook (useMicState, etc.).
    fn alloc_atomic_hook(&mut self, name: &str, setter_name: &str, atomic_slot: u32) {
        self.state_bindings.push(StateBinding {
            name: name.to_string(),
            _setter_name: setter_name.to_string(),
            backend: StateBackend::UserAtomic { slot: atomic_slot },
        });
    }

    /// Look up a state binding by variable name.
    fn _resolve_state(&self, name: &str) -> Option<&StateBinding> {
        self.state_bindings.iter().find(|b| b.name == name)
    }

    /// First pass: collect component definitions and useState let-bindings.
    fn collect_components<'a>(&mut self, program: &Program<'a>) -> Result<(), String> {
        for stmt in &program.body {
            match stmt {
                Statement::VariableDeclaration(decl) => {
                    for declarator in &decl.declarations {
                        self.try_collect_component(declarator)?;
                        self.try_collect_use_state(declarator);
                    }
                }
                // Import statements are collected but only resolved when ImportMap is provided
                _ => {}
            }
        }
        Ok(())
    }

    /// Collect import statements and resolve them against an ImportMap.
    fn collect_imports<'a>(&mut self, program: &Program<'a>, import_map: &ImportMap) {
        for stmt in &program.body {
            if let Statement::ImportDeclaration(import) = stmt {
                let source = import.source.value.as_str();
                if let Some(pkg_map) = import_map.packages.get(source) {
                    // Named imports: import { A, B } from "path"
                    if let Some(specifiers) = &import.specifiers {
                        for specifier in specifiers.iter() {
                            if let ImportDeclarationSpecifier::ImportSpecifier(spec) = specifier {
                                let local_name = spec.local.name.to_string();
                                let imported_name = match &spec.imported {
                                    ModuleExportName::Identifier(id) => id.name.to_string(),
                                    _ => local_name.clone(),
                                };
                                if let Some(oid) = pkg_map.get(&imported_name) {
                                    self.external_components.insert(local_name, *oid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Recognize `const [name, setName] = useState(initialValue)` let-bindings.
    fn try_collect_use_state<'a>(&mut self, declarator: &VariableDeclarator<'a>) {
        // LHS must be array destructuring: [name, setName]
        let elements = match &declarator.id.kind {
            BindingPatternKind::ArrayPattern(arr) => &arr.elements,
            _ => return,
        };
        if elements.len() != 2 { return; }

        let name = match &elements[0] {
            Some(pat) => match &pat.kind {
                BindingPatternKind::BindingIdentifier(id) => id.name.to_string(),
                _ => return,
            },
            None => return,
        };
        let setter_name = match &elements[1] {
            Some(pat) => match &pat.kind {
                BindingPatternKind::BindingIdentifier(id) => id.name.to_string(),
                _ => return,
            },
            None => return,
        };

        // RHS must be useState(initialValue)
        let init = match &declarator.init {
            Some(init) => init,
            None => return,
        };
        let call = match init {
            Expression::CallExpression(call) => call,
            _ => return,
        };
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        match callee_name {
            "useState" => {
                let initial = if let Some(arg) = call.arguments.first() {
                    match arg {
                        Argument::BooleanLiteral(b) => if b.value { 1 } else { 0 },
                        Argument::NumericLiteral(n) => n.value as i64,
                        _ => 0,
                    }
                } else {
                    0
                };
                self.alloc_int_state(&name, &setter_name, initial);
                return;
            }
            "useMicState" => {
                // Builtin hook: reads from UserAtomicReadable[0]
                self.alloc_atomic_hook(&name, &setter_name, 0);
                return;
            }
            _ => return,
        }
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

        self.components.insert(name, ComponentDef { _params: params, body: body_nodes });
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

        // Check if this is a composite component (inline)
        if let Some(comp) = self.components.get(&tag) {
            return self.expand_component(comp, &props, ctx);
        }

        // Check if this is an external (imported) component
        if let Some(oid) = self.external_components.get(&tag) {
            let oid_u128 = oid.to_u128();
            return Ok(vec![JsxNode {
                tag: format!("__ext:{}", oid_u128),
                props,
                children: Vec::new(),
            }]);
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

/// Build a TkvTemplate from external component props.
/// Props are sorted alphabetically (canonicalized), each assigned a sequential TkvKey.
fn build_ext_props_template(
    asm: &mut Asm,
    oid_val: u128,
    props: &[(String, PropValue)],
) -> TkvTemplate {
    // Sort props alphabetically for canonical ordering
    let mut sorted: Vec<(String, PropValue)> = props.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut entries = Vec::with_capacity(sorted.len());
    for (i, (key_name, value)) in sorted.iter().enumerate() {
        // TkvKey: sequential segments starting at 1 (segment 0 = 0 is unused by convention)
        let seg = (i + 1) as u8;
        let (tkv_type, val_bytes) = match value {
            PropValue::Str(s) => {
                let str_id = asm.def_string(s);
                let mut v = [0u8; 8];
                v[0] = StrRefDisc::StringTable as u8;
                v[1..5].copy_from_slice(&str_id.0.to_le_bytes());
                (TkvType::String, v)
            }
            PropValue::Num(n) => {
                // Heuristic: 0 or 1 with a boolean-ish name → boolean
                if (*n == 0 || *n == 1) && is_boolean_prop(key_name) {
                    let mut v = [0u8; 8];
                    v[0] = if *n != 0 { 1 } else { 0 };
                    (TkvType::Boolean, v)
                } else {
                    (TkvType::Integer, (*n as u64).to_le_bytes())
                }
            }
        };

        let key = TkvKey::new(&[seg], tkv_type);
        let key_name_id = asm.def_string(key_name);

        entries.push(TkvFixedEntry {
            key_path: key.raw(),
            value_type: tkv_type as u8,
            value: val_bytes,
            key_str_disc: StrRefDisc::StringTable as u8,
            key_str_idx: key_name_id.0 as u16,
        });
    }

    // Entries are already sorted by TkvKey since we assigned sequential segments
    // to alphabetically-sorted props
    TkvTemplate {
        oid: Oid::from_u128(oid_val),
        entries,
    }
}

/// Heuristic: prop names that are likely booleans.
fn is_boolean_prop(name: &str) -> bool {
    matches!(name,
        "required" | "optional" | "disabled" | "enabled" | "hidden" |
        "visible" | "checked" | "selected" | "readonly" | "fts" | "vec"
    )
}

fn emit_nodes(asm: &mut Asm, nodes: &[JsxNode]) {
    for node in nodes {
        emit_node(asm, node);
    }
}

/// Estimate text width for alignment: monospace approximation.
fn text_width(label: &str, size: u32) -> i32 {
    (label.len() as f32 * size as f32 * 0.6) as i32
}

/// Measure the child block size for a given layout mode.
/// Returns (block_w, block_h).
fn measure_children_block(children: &[JsxNode], layout: &str, gap: i32) -> (i32, i32) {
    let mut block_w: i32 = 0;
    let mut block_h: i32 = 0;

    for (i, child) in children.iter().enumerate() {
        let (cw, ch) = measure_node(child);
        let gap_val = if i > 0 { gap } else { 0 };
        match layout {
            "vstack" => {
                block_w = block_w.max(cw);
                block_h += ch + gap_val;
            }
            "hstack" => {
                block_w += cw + gap_val;
                block_h = block_h.max(ch);
            }
            _ => {
                // absolute / flat: bounding box
                block_w = block_w.max(cw);
                block_h = block_h.max(ch);
            }
        }
    }
    (block_w, block_h)
}

/// Measure a single node's (w, h) from its props. For Text, infer from label/size.
fn measure_node(node: &JsxNode) -> (i32, i32) {
    match node.tag.as_str() {
        "Text" => {
            let size = get_num_prop(&node.props, "size").unwrap_or(14) as u32;
            let w = get_num_prop(&node.props, "w")
                .map(|v| v as i32)
                .unwrap_or_else(|| {
                    let label = get_str_prop(&node.props, "label").unwrap_or_default();
                    text_width(&label, size)
                });
            let h = get_num_prop(&node.props, "h")
                .map(|v| v as i32)
                .unwrap_or(size as i32);
            (w, h)
        }
        _ => {
            let w = get_num_prop(&node.props, "w").unwrap_or(0) as i32;
            let h = get_num_prop(&node.props, "h").unwrap_or(0) as i32;
            (w, h)
        }
    }
}

/// Emit children with layout, padding, gap, and alignment.
fn emit_container_children(asm: &mut Asm, node: &JsxNode, parent_x: i32, parent_y: i32) {
    if node.children.is_empty() {
        return;
    }
    let layout = get_str_prop(&node.props, "layout").unwrap_or_default();
    let layout = if layout.is_empty() { "absolute" } else { &layout };
    let padding = get_num_prop(&node.props, "padding").unwrap_or(0) as i32;
    let gap = get_num_prop(&node.props, "gap").unwrap_or(0) as i32;
    let container_w = get_num_prop(&node.props, "w").unwrap_or(0) as i32;
    let container_h = get_num_prop(&node.props, "h").unwrap_or(0) as i32;
    let halign = get_str_prop(&node.props, "halign").unwrap_or_default();
    let valign = get_str_prop(&node.props, "valign").unwrap_or_default();

    // Compute child block size for alignment
    let (block_w, block_h) = measure_children_block(&node.children, layout, gap);
    let inner_w = container_w - 2 * padding;
    let inner_h = container_h - 2 * padding;

    let align_dx = match halign.as_str() {
        "center" => (inner_w - block_w) / 2,
        "right" => inner_w - block_w,
        _ => 0,
    };
    let align_dy = match valign.as_str() {
        "center" => (inner_h - block_h) / 2,
        "bottom" => inner_h - block_h,
        _ => 0,
    };

    asm.ui_push_state();
    asm.ui_apply_offset(
        parent_x + padding + align_dx,
        parent_y + padding + align_dy,
    );

    match layout {
        "vstack" => {
            let mut cursor_y: i32 = 0;
            for child in &node.children {
                let (_, ch) = measure_node(child);
                asm.ui_push_state();
                asm.ui_apply_offset(0, cursor_y);
                emit_node(asm, child);
                asm.ui_pop_state();
                cursor_y += ch + gap;
            }
        }
        "hstack" => {
            let mut cursor_x: i32 = 0;
            for child in &node.children {
                let (cw, _) = measure_node(child);
                asm.ui_push_state();
                asm.ui_apply_offset(cursor_x, 0);
                emit_node(asm, child);
                asm.ui_pop_state();
                cursor_x += cw + gap;
            }
        }
        "flat" => {
            // All children at (0,0) — overlay
            emit_nodes(asm, &node.children);
        }
        _ => {
            // "absolute" — children use own x,y
            emit_nodes(asm, &node.children);
        }
    }

    asm.ui_pop_state();
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
            if !node.children.is_empty() {
                emit_container_children(asm, node, x, y);
                return;
            }
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
            if let Some(action) = get_str_prop(&node.props, "action") {
                let action_id = asm.def_string(&action);
                asm.draw_action(x, y, w, h, action_id);
            }
            if !node.children.is_empty() {
                emit_container_children(asm, node, x, y);
                return;
            }
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
            if let Some(action) = get_str_prop(&node.props, "action") {
                let action_id = asm.def_string(&action);
                asm.draw_action(x - r as i32, y - r as i32, r * 2, r * 2, action_id);
            }
            if !node.children.is_empty() {
                emit_container_children(asm, node, x, y);
                return;
            }
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
        // ── Ribbon view: scrollable card container ──────────────────
        "RibbonView" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let w = get_num_prop(&node.props, "w").unwrap_or(360) as u32;
            let h = get_num_prop(&node.props, "h").unwrap_or(400) as u32;
            let scroll_bank = get_num_prop(&node.props, "scrollBank").unwrap_or(0) as u32;
            let card_width = get_num_prop(&node.props, "cardWidth").unwrap_or(w as i64) as u32;
            let scroll_dir = if get_str_prop(&node.props, "direction").as_deref() == Some("vertical") { 1u32 } else { 0u32 };
            asm.ui_ribbon_begin(x, y, w, h, scroll_bank, scroll_dir, card_width);
            asm.ui_push_state();
            asm.ui_apply_offset(x, y);
            for child in &node.children {
                emit_node(asm, child);
            }
            asm.ui_pop_state();
            asm.ui_ribbon_end();
            return;
        }
        // ── VQL0: Query / Vesicle tags ────────────────────────────────
        "Query" => {
            asm.set_output_mode(FOURCC_VQL0);
            asm.vql_begin_query();
            // Emit entity/filter as fields from props
            if let Some(entity) = get_str_prop(&node.props, "entity") {
                let name_id = asm.def_string("entity");
                let val_id = asm.def_string(&entity);
                asm.vql_set_field_str(name_id, val_id);
            }
            if let Some(filter) = get_str_prop(&node.props, "filter") {
                let id = asm.def_string(&filter);
                asm.vql_filter(id);
            }
            emit_nodes(asm, &node.children);
            asm.vql_end_query();
            asm.set_output_mode(FOURCC_MTUI);
            return;
        }
        "Bind" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let value = get_str_prop(&node.props, "value").unwrap_or_default();
            let name_id = asm.def_string(&name);
            let val_id = asm.def_string(&value);
            asm.vql_bind(name_id, val_id);
        }
        "Field" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let id = asm.def_string(&name);
            asm.vql_project(id);
        }
        "Filter" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let id = asm.def_string(&name);
            asm.vql_filter(id);
        }
        "Param" => {
            let key = get_str_prop(&node.props, "name").unwrap_or_default();
            let value = get_str_prop(&node.props, "value").unwrap_or_default();
            let key_id = asm.def_string(&key);
            let val_id = asm.def_string(&value);
            asm.vql_param(key_id, val_id);
        }
        // ── SKLL: Skill tags ─────────────────────────────────────────
        "Skill" => {
            asm.set_output_mode(FOURCC_SKLL);
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let name_id = asm.def_string(&name);
            asm.skill_begin(name_id);
            // Optional description props
            if let Some(short) = get_str_prop(&node.props, "shortDescription") {
                let id = asm.def_string(&short);
                asm.skill_set_short_desc(id);
            }
            if let Some(long) = get_str_prop(&node.props, "longDescription") {
                let id = asm.def_string(&long);
                asm.skill_set_long_desc(id);
            }
            emit_nodes(asm, &node.children);
            asm.skill_end();
            asm.set_output_mode(FOURCC_MTUI);
            return;
        }
        "Step" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            // If action prop present, emit as name (action is the step identity)
            let step_name = get_str_prop(&node.props, "action").unwrap_or(name);
            let id = asm.def_string(&step_name);
            asm.skill_step(id);
        }
        "LlmStep" => {
            let prompt = get_str_prop(&node.props, "prompt").unwrap_or_default();
            let prompt_id = asm.def_string(&prompt);
            asm.skill_llm_step(prompt_id);
            // Optional model attribute
            if let Some(model) = get_str_prop(&node.props, "model") {
                let model_id = asm.def_string(&model);
                asm.skill_llm_model(model_id);
            }
            // Optional useCase attribute
            if let Some(uc_str) = get_str_prop(&node.props, "useCase") {
                if let Some(uc) = LlmUseCase::from_str(&uc_str) {
                    asm.skill_llm_use_case(uc as u8);
                }
            }
            // Emit children (Replaceable elements)
            emit_nodes(asm, &node.children);
            return;
        }
        "Replaceable" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let default = get_str_prop(&node.props, "default").unwrap_or_default();
            let name_id = asm.def_string(&name);
            let default_id = asm.def_string(&default);
            asm.skill_replaceable(name_id, default_id);
        }
        "Card" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let name_id = asm.def_string(&name);
            asm.card_begin(name_id);
            if let Some(short) = get_str_prop(&node.props, "shortDescription") {
                let id = asm.def_string(&short);
                asm.card_set_short_desc(id);
            }
            if let Some(long) = get_str_prop(&node.props, "longDescription") {
                let id = asm.def_string(&long);
                asm.card_set_long_desc(id);
            }
            // Children are UI elements (Box, Slab, Text, etc.) captured into the card
            emit_nodes(asm, &node.children);
            asm.card_end();
            return;
        }
        "ObjectType" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let name_id = asm.def_string(&name);
            asm.objtype_begin(name_id);
            if let Some(short) = get_str_prop(&node.props, "shortDescription") {
                let id = asm.def_string(&short);
                asm.objtype_set_short_desc(id);
            }
            if let Some(long) = get_str_prop(&node.props, "longDescription") {
                let id = asm.def_string(&long);
                asm.objtype_set_long_desc(id);
            }
            emit_nodes(asm, &node.children);
            asm.objtype_end();
            return;
        }
        "TypeField" => {
            let name = get_str_prop(&node.props, "name").unwrap_or_default();
            let name_id = asm.def_string(&name);
            let fts = get_str_prop(&node.props, "fts").map(|v| v == "true" || v == "1").unwrap_or(false);
            let vec = get_str_prop(&node.props, "vec").map(|v| v == "true" || v == "1").unwrap_or(false);
            let flags: u32 = (fts as u32) | ((vec as u32) << 1);
            asm.objtype_field(name_id, flags);
        }
        "Cron" => {
            if let Some(interval) = get_num_prop(&node.props, "interval") {
                asm.skill_cron_interval(interval as u64);
            }
            if let Some(jitter) = get_num_prop(&node.props, "jitter") {
                asm.skill_cron_jitter(jitter as u64);
            }
        }
        "Invoke" => {
            let action = get_str_prop(&node.props, "action").unwrap_or_default();
            if let Some(symbol) = get_num_prop(&node.props, "symbol") {
                asm.skill_invoke_symbol(symbol as u32);
            } else {
                let id = asm.def_string(&action);
                asm.skill_invoke(id);
            }
        }
        "ForwardPrompt" => {
            let dest = get_str_prop(&node.props, "dest").unwrap_or_else(|| "thinker".to_string());
            let id = asm.def_string(&dest);
            asm.skill_forward_prompt(id);
        }
        "AddToSystemPrompt" => {
            let content = get_str_prop(&node.props, "content").unwrap_or_default();
            let id = asm.def_string(&content);
            asm.skill_add_to_system_prompt(id);
        }
        "VStack" => {
            // VStack is sugar for Box layout="vstack" (no visual, just layout)
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            // Create a synthetic node with layout="vstack" for emit_container_children
            let mut vstack_node = node.clone();
            // Ensure layout is set to vstack
            if get_str_prop(&vstack_node.props, "layout").is_none() {
                vstack_node.props.push(("layout".to_string(), PropValue::Str("vstack".to_string())));
            }
            emit_container_children(asm, &vstack_node, x, y);
            return; // children already emitted
        }
        "HStack" => {
            // HStack is sugar for Box layout="hstack" (no visual, just layout)
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let mut hstack_node = node.clone();
            if get_str_prop(&hstack_node.props, "layout").is_none() {
                hstack_node.props.push(("layout".to_string(), PropValue::Str("hstack".to_string())));
            }
            emit_container_children(asm, &hstack_node, x, y);
            return;
        }
        "Ribbon" => {
            let x = get_num_prop(&node.props, "x").unwrap_or(0) as i32;
            let y = get_num_prop(&node.props, "y").unwrap_or(0) as i32;
            let w = get_num_prop(&node.props, "w").unwrap_or(400) as u32;
            let h = get_num_prop(&node.props, "h").unwrap_or(300) as u32;
            let scroll_slot = get_num_prop(&node.props, "scrollSlot").unwrap_or(0) as u32;
            let direction = get_num_prop(&node.props, "direction").unwrap_or(0) as u32;
            let card_width = get_num_prop(&node.props, "cardWidth").unwrap_or(360) as u32;
            asm.ui_ribbon_begin(x, y, w, h, scroll_slot, direction, card_width);
            emit_nodes(asm, &node.children);
            asm.ui_ribbon_end();
            return;
        }
        _ => {
            // Check for external component (__ext:OID)
            if node.tag.starts_with("__ext:") {
                let oid_str = &node.tag[6..];
                if let Ok(oid_val) = oid_str.parse::<u128>() {
                    // Build TKV template from props (sorted, canonicalized)
                    if !node.props.is_empty() {
                        let template = build_ext_props_template(asm, oid_val, &node.props);
                        let template_idx = asm.tkv_static_table.len() as u32;
                        asm.tkv_static_table.push(template);
                        // Emit: push template index → UserCall(TKV, CLONE)
                        asm.push32(template_idx);
                        asm.user_call(
                            matterstream_vm_asm::user_call::TKV,
                            matterstream_vm_addressing::tkv_ops::TkvOp::Clone as u64,
                        );
                    }
                    // Push OID and dispatch — NativeHook or Component
                    asm.push128(oid_val);
                    asm.oid_call();
                    return;
                }
            }
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
/// Compile TSX source with import resolution.
///
/// Import statements (`import { X } from "path"`) are resolved against the
/// ImportMap. External components emit OidImport + ExecComponent bytecode
/// instead of inline expansion.
pub fn compile_to_asm_with_imports(tsx_source: &str, imports: &ImportMap) -> Result<AsmOutput, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path("card.tsx")
        .map_err(|e| format!("SourceType error: {:?}", e))?;
    let ret = Parser::new(&allocator, tsx_source, source_type).parse();

    if !ret.errors.is_empty() {
        let msgs: Vec<String> = ret.errors.into_iter().map(|e| format!("{:?}", e)).collect();
        return Err(msgs.join("\n"));
    }

    let mut compiler = AsmCompiler::new();
    compiler.collect_components(&ret.program)?;
    compiler.collect_imports(&ret.program, imports);

    let nodes = compiler.lower_program(&ret.program)?;
    let mut asm = Asm::new();

    for binding in &compiler.state_bindings {
        match &binding.backend {
            StateBackend::IntBank { packed_ref, initial_value } => {
                let bank_type = (packed_ref >> 16) as u32;
                let slot = (packed_ref & 0xFFFF) as u32;
                asm.push32(*initial_value as u32);
                asm.push32(bank_type);
                asm.push32(slot);
                asm.op(matterstream_vm::rpn::RpnOp::StoreBank);
            }
            StateBackend::UserAtomic { slot } => {
                asm.read_user_atomic(*slot);
                let local_bank = BANK_INT as u32;
                let local_slot = (compiler.state_bindings.iter()
                    .position(|b| b.name == binding.name)
                    .unwrap()) as u32;
                asm.push32(local_bank);
                asm.push32(local_slot);
                asm.op(matterstream_vm::rpn::RpnOp::StoreBank);
            }
        }
    }

    emit_nodes(&mut asm, &nodes);
    asm.halt();

    asm.finish().map_err(|e| format!("asm error: {:?}", e))
}

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

    // Emit state initialization for let-bindings
    for binding in &compiler.state_bindings {
        match &binding.backend {
            StateBackend::IntBank { packed_ref, initial_value } => {
                let bank_type = (packed_ref >> 16) as u32;
                let slot = (packed_ref & 0xFFFF) as u32;
                asm.push32(*initial_value as u32);
                asm.push32(bank_type);
                asm.push32(slot);
                asm.op(matterstream_vm::rpn::RpnOp::StoreBank);
            }
            StateBackend::UserAtomic { slot } => {
                // Read UserAtomicReadable[slot] → IntBank[local_slot]
                // Runs every frame (bytecode is re-executed for UI)
                asm.read_user_atomic(*slot);
                // Store result to a local IntBank slot
                let local_bank = BANK_INT as u32;
                let local_slot = (compiler.state_bindings.iter()
                    .position(|b| b.name == binding.name)
                    .unwrap()) as u32;
                asm.push32(local_bank);
                asm.push32(local_slot);
                asm.op(matterstream_vm::rpn::RpnOp::StoreBank);
            }
        }
    }

    emit_nodes(&mut asm, &nodes);
    asm.halt();

    asm.finish().map_err(|e| format!("asm error: {:?}", e))
}
