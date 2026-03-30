//! SKLL OR page handler — extracts SkillDef outputs from VM execution.
//!
//! Register with `vm.setup().register_or_page(FOURCC_SKLL, Box::new(SkllHandler::new()))`,
//! execute bytecode, then `vm.or_page_handle::<SkllHandler>(FOURCC_SKLL)` to read outputs.

use std::any::Any;
use matterstream_vm::or_page::OrPageHandler;
use matterstream_vm::rpn::RpnError;
use matterstream_vm::ui_vm::{
    CronSpec, LlmUseCase, SkillDef, SkillReplaceable, SkillStep,
};
use matterstream_vm::vm_handle::VmHandle;
use matterstream_vm_arena::TripleArena;

pub struct SkllHandler {
    pub outputs: Vec<SkillDef>,
    active: Option<SkillDef>,
    active_llm_prompt: Option<String>,
    active_llm_replaceables: Vec<SkillReplaceable>,
    active_llm_model: Option<String>,
    active_llm_use_case: LlmUseCase,
}

impl SkllHandler {
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            active: None,
            active_llm_prompt: None,
            active_llm_replaceables: Vec::new(),
            active_llm_model: None,
            active_llm_use_case: LlmUseCase::default(),
        }
    }

    fn finish_llm_step(&mut self) -> Result<(), RpnError> {
        if let Some(prompt) = self.active_llm_prompt.take() {
            let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
            skill.steps.push(SkillStep::Llm {
                prompt,
                replaceables: std::mem::take(&mut self.active_llm_replaceables),
                model: self.active_llm_model.take(),
                use_case: self.active_llm_use_case,
            });
            self.active_llm_use_case = LlmUseCase::default();
        }
        Ok(())
    }
}

impl OrPageHandler for SkllHandler {
    fn dispatch(
        &mut self,
        sub_op: u8,
        vm: &mut VmHandle,
        _arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        match sub_op {
            0x00 => { // Begin
                let idx = vm.pop_u32()?;
                let name = vm.resolve_str(idx)?;
                self.active = Some(SkillDef::new(name));
            }
            0x01 => { // End
                self.finish_llm_step()?;
                let skill = self.active.take().ok_or(RpnError::SkillNoActiveDef)?;
                self.outputs.push(skill);
            }
            0x02 => { // Step
                self.finish_llm_step()?;
                let idx = vm.pop_u32()?;
                let name = vm.resolve_str(idx)?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::Deterministic { name });
            }
            0x03 => { // LlmStep
                self.finish_llm_step()?;
                let idx = vm.pop_u32()?;
                let prompt = vm.resolve_str(idx)?;
                self.active_llm_prompt = Some(prompt);
            }
            0x04 => { // Replaceable
                let default_idx = vm.pop_u32()?;
                let name_idx = vm.pop_u32()?;
                let name = vm.resolve_str(name_idx)?;
                let default = vm.resolve_str(default_idx)?;
                self.active_llm_replaceables.push(SkillReplaceable { name, default });
            }
            0x05 => { // Invoke
                self.finish_llm_step()?;
                let idx = vm.pop_u32()?;
                let name = vm.resolve_str(idx)?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::InvokeAction { name });
            }
            0x06 => { // InvokeSymbol
                self.finish_llm_step()?;
                let symbol = vm.pop_u32()?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::InvokeSymbol { symbol });
            }
            0x07 => { // LlmModel
                let idx = vm.pop_u32()?;
                let model = vm.resolve_str(idx)?;
                self.active_llm_model = Some(model);
            }
            0x08 => { // LlmUseCase
                let val = vm.pop_u32()? as u8;
                self.active_llm_use_case = LlmUseCase::from_u8(val).unwrap_or_default();
            }
            0x09 => { // SetShortDesc
                let idx = vm.pop_u32()?;
                let desc = vm.resolve_str(idx)?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.short_description = desc;
            }
            0x0A => { // SetLongDesc
                let idx = vm.pop_u32()?;
                let desc = vm.resolve_str(idx)?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.long_description = desc;
            }
            0x0B => { // CronInterval
                let val = vm.pop_u32()? as u64;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                let cron = skill.cron.get_or_insert(CronSpec { interval_ms: 0, jitter_ms: 0 });
                cron.interval_ms = val;
            }
            0x0C => { // CronJitter
                let val = vm.pop_u32()? as u64;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                let cron = skill.cron.get_or_insert(CronSpec { interval_ms: 0, jitter_ms: 0 });
                cron.jitter_ms = val;
            }
            0x0D => { // ForwardPrompt
                self.finish_llm_step()?;
                let idx = vm.pop_u32()?;
                let dest = vm.resolve_str(idx)?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::ForwardPrompt { dest });
            }
            0x0E => { // AddToSystemPrompt
                self.finish_llm_step()?;
                let idx = vm.pop_u32()?;
                let content = vm.resolve_str(idx)?;
                let skill = self.active.as_mut().ok_or(RpnError::SkillNoActiveDef)?;
                skill.steps.push(SkillStep::AddToSystemPrompt { content });
            }
            _ => {}
        }
        Ok(())
    }

    fn gas_cost(&self, _sub_op: u8) -> u64 { 100 }

    fn as_any(self: Box<Self>) -> Box<dyn Any> { self }
    fn as_any_ref(&self) -> &dyn Any { self }
}
