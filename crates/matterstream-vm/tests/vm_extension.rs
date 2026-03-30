//! Tests for VM extension: control registers, VQL0, and external OR page dispatch.

use std::any::Any;
use matterstream_vm::rpn::{RpnOp, RpnVm, RpnValue, RpnError, VqlOp};
use matterstream_vm::or_page::OrPageHandler;
use matterstream_vm::vm_handle::VmHandle;
use matterstream_vm::ui_vm::{
    VqlField, LlmUseCase,
    CR_OUTPUT_MODE, FOURCC_MTUI, FOURCC_VQL0,
};
use matterstream_vm_arena::TripleArena;

fn make_vm_with_strings(strings: &[&str]) -> RpnVm {
    let mut vm = RpnVm::new();
    for s in strings {
        vm.string_table.push(s.to_string());
    }
    vm
}

/// Emit inline SetCR: [opcode][cr_idx: u8][value: u64 LE]
fn emit_set_cr(bc: &mut Vec<u8>, cr_idx: u8, value: u64) {
    bc.push(RpnOp::SetCR as u8);
    bc.push(cr_idx);
    bc.extend_from_slice(&value.to_le_bytes());
}

fn push32(bc: &mut Vec<u8>, val: u32) {
    bc.push(RpnOp::Push32 as u8);
    bc.extend_from_slice(&val.to_le_bytes());
}

fn push64(bc: &mut Vec<u8>, val: u64) {
    bc.push(RpnOp::Push64 as u8);
    bc.extend_from_slice(&val.to_le_bytes());
}

// ── Control Register tests ──────────────────────────────────────────────

#[test]
fn test_set_cr_output_mode() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    // SetCR: inline cr_index=0, value=FOURCC_VQL0
    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.cr_bank[CR_OUTPUT_MODE], FOURCC_VQL0);
}

#[test]
fn test_set_cr_invalid_index() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 99, 42);
    bytecode.push(RpnOp::Halt as u8);

    let result = vm.execute(&bytecode, &mut arena);
    assert!(result.is_err());
}

#[test]
fn test_cr_resets_on_execute() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    // Set CR0 to VQL0
    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    bytecode.push(RpnOp::Halt as u8);
    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.cr_bank[CR_OUTPUT_MODE], FOURCC_VQL0);

    // Execute again — should reset to MTUI
    let bytecode2 = vec![RpnOp::Halt as u8];
    vm.execute(&bytecode2, &mut arena).unwrap();
    assert_eq!(vm.cr_bank[CR_OUTPUT_MODE], FOURCC_MTUI);
}

// ── VQL0 tests ──────────────────────────────────────────────────────────

#[test]
fn test_vql_basic_query() {
    // String table: 0="email", 1="active"
    let mut vm = make_vm_with_strings(&["email", "active"]);
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    bytecode.push(VqlOp::BeginQuery.byte());
    // VqlProject "email" (str_idx=0)
    push32(&mut bytecode, 0);
    bytecode.push(VqlOp::Project.byte());
    // VqlFilter "active" (str_idx=1)
    push32(&mut bytecode, 1);
    bytecode.push(VqlOp::Filter.byte());
    bytecode.push(VqlOp::EndQuery.byte());
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.vql_outputs.len(), 1);
    assert_eq!(vm.vql_outputs[0].fields.len(), 2);
    assert_eq!(vm.vql_outputs[0].fields[0], VqlField::Project("email".into()));
    assert_eq!(vm.vql_outputs[0].fields[1], VqlField::Filter("active".into()));
}

#[test]
fn test_vql_bind_and_param() {
    // String table: 0="role", 1="admin", 2="limit", 3="100"
    let mut vm = make_vm_with_strings(&["role", "admin", "limit", "100"]);
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    bytecode.push(VqlOp::BeginQuery.byte());
    // VqlBind "role" = "admin"
    push32(&mut bytecode, 0);
    push32(&mut bytecode, 1);
    bytecode.push(VqlOp::Bind.byte());
    // VqlParam "limit" = "100"
    push32(&mut bytecode, 2);
    push32(&mut bytecode, 3);
    bytecode.push(VqlOp::Param.byte());
    bytecode.push(VqlOp::EndQuery.byte());
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.vql_outputs.len(), 1);
    assert_eq!(
        vm.vql_outputs[0].fields[0],
        VqlField::Bind { name: "role".into(), value: "admin".into() }
    );
    assert_eq!(
        vm.vql_outputs[0].fields[1],
        VqlField::Param { key: "limit".into(), value: "100".into() }
    );
}

#[test]
fn test_vql_set_field_numeric() {
    let mut vm = make_vm_with_strings(&["count"]);
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    bytecode.push(VqlOp::BeginQuery.byte());
    // VqlSetField "count" = 42
    push32(&mut bytecode, 0);  // name_idx
    push64(&mut bytecode, 42);  // value
    bytecode.push(VqlOp::SetField.byte());
    bytecode.push(VqlOp::EndQuery.byte());
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(
        vm.vql_outputs[0].fields[0],
        VqlField::FieldValue { name: "count".into(), value: 42 }
    );
}

#[test]
fn test_vql_no_active_query_error() {
    let mut vm = make_vm_with_strings(&["x"]);
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    push32(&mut bytecode, 0);
    bytecode.push(VqlOp::Project.byte());
    bytecode.push(RpnOp::Halt as u8);

    assert!(vm.execute(&bytecode, &mut arena).is_err());
}

#[test]
fn test_vql_multiple_queries() {
    let mut vm = make_vm_with_strings(&["a", "b"]);
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_VQL0 as u64);
    bytecode.push(VqlOp::BeginQuery.byte());
    push32(&mut bytecode, 0);
    bytecode.push(VqlOp::Project.byte());
    bytecode.push(VqlOp::EndQuery.byte());
    bytecode.push(VqlOp::BeginQuery.byte());
    push32(&mut bytecode, 1);
    bytecode.push(VqlOp::Project.byte());
    bytecode.push(VqlOp::EndQuery.byte());
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.vql_outputs.len(), 2);
}

// ── External OR page handler tests ──────────────────────────────────────

const FOURCC_TEST: u32 = 0x54455354; // "TEST"

/// Simple test handler that records sub_ops and pops values from the stack.
struct TestPageHandler {
    dispatched: Vec<u8>,
    popped_values: Vec<u32>,
}

impl TestPageHandler {
    fn new() -> Self {
        Self {
            dispatched: Vec::new(),
            popped_values: Vec::new(),
        }
    }
}

impl OrPageHandler for TestPageHandler {
    fn dispatch(
        &mut self,
        sub_op: u8,
        vm: &mut VmHandle,
        _arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        self.dispatched.push(sub_op);
        // Sub-op 0x00: pop a u32 and record it
        if sub_op == 0x00 {
            let val = vm.pop_u32()?;
            self.popped_values.push(val);
        }
        // Sub-op 0x01: push a result onto the stack
        if sub_op == 0x01 {
            vm.push(RpnValue::U32(0xBEEF))?;
        }
        // Sub-op 0xFF: return error
        if sub_op == 0x7F {
            return Err(RpnError::InvalidOrPage(FOURCC_TEST));
        }
        Ok(())
    }

    fn gas_cost(&self, sub_op: u8) -> u64 {
        match sub_op {
            0x00 => 50,
            0x01 => 25,
            _ => 0, // use default
        }
    }

    fn as_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}

#[test]
fn test_external_or_page_dispatch() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    vm.setup().register_or_page(FOURCC_TEST, Box::new(TestPageHandler::new()));

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_TEST as u64);
    // Push a value, then dispatch sub-op 0x00 (which pops it)
    push32(&mut bytecode, 42);
    bytecode.push(0x80); // OR page sub-op 0x00
    // Dispatch sub-op 0x01 (which pushes 0xBEEF)
    bytecode.push(0x81); // OR page sub-op 0x01
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();

    // Sub-op 0x00 popped 42, sub-op 0x01 pushed 0xBEEF
    assert_eq!(vm.stack.len(), 1);
    assert_eq!(vm.stack[0].as_u32(), Some(0xBEEF));
}

#[test]
fn test_external_or_page_unregistered_fourcc() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_TEST as u64);
    bytecode.push(0x80); // OR page sub-op 0x00
    bytecode.push(RpnOp::Halt as u8);

    // Should error — no handler registered for FOURCC_TEST
    assert!(vm.execute(&bytecode, &mut arena).is_err());
}

#[test]
fn test_external_or_page_gas_metering() {
    let mut vm = RpnVm::with_gas(200);
    let mut arena = TripleArena::new();

    vm.setup().register_or_page(FOURCC_TEST, Box::new(TestPageHandler::new()));

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_TEST as u64);
    // Sub-op 0x01 costs 25 gas each, dispatch 5 times = 125 gas
    for _ in 0..5 {
        bytecode.push(0x81);
    }
    bytecode.push(RpnOp::Halt as u8);

    // Should succeed — 125 + overhead < 200
    vm.execute(&bytecode, &mut arena).unwrap();
    assert!(vm.trace.gas_consumed > 0);
}

#[test]
fn test_external_or_page_string_resolution() {
    let mut vm = make_vm_with_strings(&["hello", "world"]);
    let mut arena = TripleArena::new();

    vm.setup().register_or_page(FOURCC_TEST, Box::new(TestPageHandler::new()));

    let mut bytecode = Vec::new();
    emit_set_cr(&mut bytecode, 0, FOURCC_TEST as u64);
    // Push string index 0, then sub-op 0x00 pops it
    push32(&mut bytecode, 0);
    bytecode.push(0x80);
    bytecode.push(RpnOp::Halt as u8);

    vm.execute(&bytecode, &mut arena).unwrap();

    // Sub-op 0x00 popped the string index; stack should be empty
    assert_eq!(vm.stack.len(), 0);
}

// ── LlmUseCase enum tests ──────────────────────────────────────────────

#[test]
fn test_llm_use_case_from_str() {
    assert_eq!(LlmUseCase::from_str("routing"), Some(LlmUseCase::Routing));
    assert_eq!(LlmUseCase::from_str("thinking"), Some(LlmUseCase::Thinking));
    assert_eq!(LlmUseCase::from_str("deep-research"), Some(LlmUseCase::DeepResearch));
    assert_eq!(LlmUseCase::from_str("deep_research"), Some(LlmUseCase::DeepResearch));
    assert_eq!(LlmUseCase::from_str("codegen"), Some(LlmUseCase::CodeGen));
    assert_eq!(LlmUseCase::from_str("code-gen"), Some(LlmUseCase::CodeGen));
    assert_eq!(LlmUseCase::from_str("extract"), Some(LlmUseCase::Extract));
    assert_eq!(LlmUseCase::from_str("validate"), Some(LlmUseCase::Validate));
    assert_eq!(LlmUseCase::from_str("unknown"), None);
}

#[test]
fn test_llm_use_case_from_u8() {
    assert_eq!(LlmUseCase::from_u8(0), Some(LlmUseCase::General));
    assert_eq!(LlmUseCase::from_u8(1), Some(LlmUseCase::Routing));
    assert_eq!(LlmUseCase::from_u8(7), Some(LlmUseCase::Validate));
    assert_eq!(LlmUseCase::from_u8(255), None);
}
