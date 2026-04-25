//! SkillHost — trait for host callbacks from the SKLS extension.

pub trait SkillHostCallbacks: Send {
    /// Set the LLM that will continue processing.
    fn set_model(&mut self, model_name: &str);
    
    /// Add text to the system prompt.
    fn append_to_prompt(&mut self, text: &str);
    
    /// Execute a prompt and return the result.
    fn execute_prompt(&mut self, prompt: Option<&str>) -> Result<String, String>;
    
    /// Pass control to the model with the current system prompt.
    fn forward_to_model(&mut self, system_prompt: &str);
    
    /// Queue a skill to run next.
    fn queue_skill(&mut self, name: &str);
    
    /// Queue an action to run next.
    fn queue_action(&mut self, name: &str);
    
    /// Execute a specific skill by name.
    fn execute_skill(&mut self, name: &str) -> Result<(), String>;
    
    /// Execute a specific action by name and return the result.
    fn execute_action(&mut self, name: &str) -> Result<String, String>;

    // ── Ref bindings ─────────────────────────────────────────────
    //
    // TSX skills can declare `<Ref name="…" />` slots and read/write
    // them via `<SetValue ref="…">{…}</SetValue>` / `<GetValue ref>`.
    // Per-run storage lives on the host side (the SkillsHost's
    // RunningSkill keeps a `HashMap<String, String>`) so the VM
    // layer stays ignorant of lifetime concerns.
    //
    // Default impls are no-ops so older `SkillHostCallbacks`
    // implementations continue to compile — useful for embedded
    // demos that don't care about ref bindings.

    /// Declare a ref slot. Called when the VM hits a `<Ref>`
    /// element; hosts typically seed the slot with its `default`
    /// value if provided.
    fn define_ref(&mut self, _name: &str, _default: Option<&str>) {}

    /// Write a value into a ref. Called by `<SetValue ref="…">`.
    /// The previous value (if any) is returned so the caller can
    /// implement undo semantics if needed. Default impl drops.
    fn set_ref(&mut self, _name: &str, _value: &str) -> Option<String> { None }

    /// Read a ref. Returns `None` if the slot was never defined nor
    /// set. Consumed by `<GetValue ref="…">` and by the `{{name}}`
    /// template-substitution path inside `execute_prompt`.
    fn get_ref(&self, _name: &str) -> Option<String> { None }
}
