//! VmSharedState — trait for externally-owned atomic and string shared state.
//!
//! Implementations are provided by the host application (e.g. `GlobalSharedState`)
//! and passed to the VM at construction. The VM's `ReadUserAtomic`,
//! `SubmitUserSemaphore`, and `SharedStringGet/Set` opcodes dispatch through
//! this trait when set, allowing live shared state between the audio pipeline,
//! render loop, and VM execution.

/// Trait for externally-owned shared state accessed by VM opcodes.
pub trait VmSharedState: Send + Sync {
    /// Read an atomic value (host → VM). Slot 0 = audio/mic state.
    fn read_atomic(&self, slot: usize) -> u32;

    /// Write a semaphore value (VM → host, fire-and-forget).
    fn write_semaphore(&self, slot: usize, val: u32);

    /// Read a shared string (returns clone).
    fn read_string(&self, slot: usize) -> Option<String>;

    /// Write a shared string.
    fn write_string(&self, slot: usize, val: Option<String>);
}
