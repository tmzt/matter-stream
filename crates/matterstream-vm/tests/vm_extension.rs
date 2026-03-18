//! Tests for VM extension: control registers, VQL0, and SKLL opcodes.

use matterstream_vm::rpn::{RpnOp, RpnVm};
use matterstream_vm::ui_vm::{
    VqlField, LlmUseCase, SkillStep,
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

// ── Control Register tests ──────────────────────────────────────────────

#[test]
fn test_set_cr_output_mode() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    // SetCR: push cr_index=0, push value=FOURCC_VQL0, SetCR
    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),       // cr_index = 0
        (RpnOp::Push32, Some(&FOURCC_VQL0.to_le_bytes())), // value
        (RpnOp::SetCR, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.cr_bank[CR_OUTPUT_MODE], FOURCC_VQL0);
}

#[test]
fn test_set_cr_invalid_index() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&99u32.to_le_bytes())),  // invalid index
        (RpnOp::Push32, Some(&42u32.to_le_bytes())),
        (RpnOp::SetCR, None),
    ]);

    let result = vm.execute(&bytecode, &mut arena);
    assert!(result.is_err());
}

#[test]
fn test_cr_resets_on_execute() {
    let mut vm = RpnVm::new();
    let mut arena = TripleArena::new();

    // Set CR0 to VQL0
    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::Push32, Some(&FOURCC_VQL0.to_le_bytes())),
        (RpnOp::SetCR, None),
        (RpnOp::Halt, None),
    ]);
    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.cr_bank[CR_OUTPUT_MODE], FOURCC_VQL0);

    // Execute again — should reset to MTUI
    let bytecode2 = RpnVm::encode(&[(RpnOp::Halt, None)]);
    vm.execute(&bytecode2, &mut arena).unwrap();
    assert_eq!(vm.cr_bank[CR_OUTPUT_MODE], FOURCC_MTUI);
}

// ── VQL0 tests ──────────────────────────────────────────────────────────

#[test]
fn test_vql_basic_query() {
    // String table: 0="email", 1="active"
    let mut vm = make_vm_with_strings(&["email", "active"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::VqlBeginQuery, None),
        // VqlProject "email" (str_idx=0)
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::VqlProject, None),
        // VqlFilter "active" (str_idx=1)
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::VqlFilter, None),
        (RpnOp::VqlEndQuery, None),
        (RpnOp::Halt, None),
    ]);

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

    let bytecode = RpnVm::encode(&[
        (RpnOp::VqlBeginQuery, None),
        // VqlBind "role" = "admin"
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::VqlBind, None),
        // VqlParam "limit" = "100"
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::VqlParam, None),
        (RpnOp::VqlEndQuery, None),
        (RpnOp::Halt, None),
    ]);

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

    let bytecode = RpnVm::encode(&[
        (RpnOp::VqlBeginQuery, None),
        // VqlSetField "count" = 42
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),  // name_idx
        (RpnOp::Push64, Some(&42u64.to_le_bytes())),  // value
        (RpnOp::VqlSetField, None),
        (RpnOp::VqlEndQuery, None),
        (RpnOp::Halt, None),
    ]);

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

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::VqlProject, None),
    ]);

    assert!(vm.execute(&bytecode, &mut arena).is_err());
}

#[test]
fn test_vql_multiple_queries() {
    let mut vm = make_vm_with_strings(&["a", "b"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::VqlBeginQuery, None),
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::VqlProject, None),
        (RpnOp::VqlEndQuery, None),
        (RpnOp::VqlBeginQuery, None),
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::VqlProject, None),
        (RpnOp::VqlEndQuery, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.vql_outputs.len(), 2);
}

// ── SKLL tests ──────────────────────────────────────────────────────────

#[test]
fn test_skill_basic() {
    // 0="onboard", 1="validate", 2="provision"
    let mut vm = make_vm_with_strings(&["onboard", "validate", "provision"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        // SkillBegin "onboard" (str_idx=0)
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        // SkillStep "validate" (str_idx=1)
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        // SkillStep "provision" (str_idx=2)
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.skill_outputs.len(), 1);
    assert_eq!(vm.skill_outputs[0].name, "onboard");
    assert_eq!(vm.skill_outputs[0].steps.len(), 2);
    assert_eq!(
        vm.skill_outputs[0].steps[0],
        SkillStep::Deterministic { name: "validate".into() }
    );
}

#[test]
fn test_skill_llm_step_with_replaceables() {
    // 0="summarize", 1="Summarize {{user}}", 2="user", 3="Unknown"
    let mut vm = make_vm_with_strings(&[
        "summarize",
        "Summarize {{user}}",
        "user",
        "Unknown",
    ]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        // LlmStep with prompt
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillLlmStep, None),
        // Replaceable: name="user", default="Unknown"
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::SkillReplaceable, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.skill_outputs.len(), 1);
    assert_eq!(vm.skill_outputs[0].steps.len(), 1);
    match &vm.skill_outputs[0].steps[0] {
        SkillStep::Llm { prompt, replaceables, model, use_case } => {
            assert_eq!(prompt, "Summarize {{user}}");
            assert_eq!(replaceables.len(), 1);
            assert_eq!(replaceables[0].name, "user");
            assert_eq!(replaceables[0].default, "Unknown");
            assert_eq!(*model, None);
            assert_eq!(*use_case, LlmUseCase::General);
        }
        _ => panic!("Expected LLM step"),
    }
}

#[test]
fn test_skill_llm_with_model_and_use_case() {
    // 0="classify", 1="Classify this", 2="claude-haiku"
    let mut vm = make_vm_with_strings(&["classify", "Classify this", "claude-haiku"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        // LlmStep
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillLlmStep, None),
        // Set model
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::SkillLlmModel, None),
        // Set use case = Routing (1)
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillLlmUseCase, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    match &vm.skill_outputs[0].steps[0] {
        SkillStep::Llm { model, use_case, .. } => {
            assert_eq!(model.as_deref(), Some("claude-haiku"));
            assert_eq!(*use_case, LlmUseCase::Routing);
        }
        _ => panic!("Expected LLM step"),
    }
}

#[test]
fn test_skill_invoke_action() {
    let mut vm = make_vm_with_strings(&["my_skill", "send_email"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillInvoke, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(
        vm.skill_outputs[0].steps[0],
        SkillStep::InvokeAction { name: "send_email".into() }
    );
}

#[test]
fn test_skill_invoke_symbol() {
    let mut vm = make_vm_with_strings(&["my_skill"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&0x42u32.to_le_bytes())),
        (RpnOp::SkillInvokeSymbol, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(
        vm.skill_outputs[0].steps[0],
        SkillStep::InvokeSymbol { symbol: 0x42 }
    );
}

#[test]
fn test_skill_no_active_def_error() {
    let mut vm = make_vm_with_strings(&["x"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
    ]);

    assert!(vm.execute(&bytecode, &mut arena).is_err());
}

#[test]
fn test_skill_mixed_steps() {
    // 0="pipeline", 1="fetch", 2="Analyze {{data}}", 3="data", 4="", 5="store"
    let mut vm = make_vm_with_strings(&[
        "pipeline", "fetch", "Analyze {{data}}", "data", "", "store",
    ]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        // Deterministic step "fetch"
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        // LLM step with replaceable
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::SkillLlmStep, None),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::Push32, Some(&4u32.to_le_bytes())),
        (RpnOp::SkillReplaceable, None),
        // Use case = DeepResearch (3)
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::SkillLlmUseCase, None),
        // Deterministic step "store" — should flush LLM step first
        (RpnOp::Push32, Some(&5u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.skill_outputs[0].steps.len(), 3);
    assert_eq!(
        vm.skill_outputs[0].steps[0],
        SkillStep::Deterministic { name: "fetch".into() }
    );
    match &vm.skill_outputs[0].steps[1] {
        SkillStep::Llm { prompt, replaceables, use_case, .. } => {
            assert_eq!(prompt, "Analyze {{data}}");
            assert_eq!(replaceables.len(), 1);
            assert_eq!(*use_case, LlmUseCase::DeepResearch);
        }
        _ => panic!("Expected LLM step"),
    }
    assert_eq!(
        vm.skill_outputs[0].steps[2],
        SkillStep::Deterministic { name: "store".into() }
    );
}

// ── Nested skill tests ──────────────────────────────────────────────────

#[test]
fn test_nested_skills() {
    // 0="parent", 1="child_a", 2="child_b", 3="step_p", 4="step_a", 5="step_b"
    let mut vm = make_vm_with_strings(&[
        "parent", "child_a", "child_b", "step_p", "step_a", "step_b",
    ]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        // Begin parent skill
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        // Nested: child_a
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&4u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        (RpnOp::SkillEnd, None),
        // Nested: child_b
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&5u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        (RpnOp::SkillEnd, None),
        // End parent skill
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    // Should have 3 skills: child_a, child_b, parent (in order of SkillEnd)
    assert_eq!(vm.skill_outputs.len(), 3);
    assert_eq!(vm.skill_outputs[0].name, "child_a");
    assert_eq!(vm.skill_outputs[0].steps.len(), 1);
    assert_eq!(vm.skill_outputs[1].name, "child_b");
    assert_eq!(vm.skill_outputs[1].steps.len(), 1);
    assert_eq!(vm.skill_outputs[2].name, "parent");
    assert_eq!(vm.skill_outputs[2].steps.len(), 1);
}

#[test]
fn test_sibling_skills() {
    // 0="alpha", 1="beta", 2="s1", 3="s2"
    let mut vm = make_vm_with_strings(&["alpha", "beta", "s1", "s2"]);
    let mut arena = TripleArena::new();

    let bytecode = RpnVm::encode(&[
        (RpnOp::Push32, Some(&0u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&2u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Push32, Some(&1u32.to_le_bytes())),
        (RpnOp::SkillBegin, None),
        (RpnOp::Push32, Some(&3u32.to_le_bytes())),
        (RpnOp::SkillStep, None),
        (RpnOp::SkillEnd, None),
        (RpnOp::Halt, None),
    ]);

    vm.execute(&bytecode, &mut arena).unwrap();
    assert_eq!(vm.skill_outputs.len(), 2);
    assert_eq!(vm.skill_outputs[0].name, "alpha");
    assert_eq!(vm.skill_outputs[1].name, "beta");
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
