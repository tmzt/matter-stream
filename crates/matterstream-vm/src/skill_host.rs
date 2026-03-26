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
}
