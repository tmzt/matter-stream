//! State Stack — shadow register stack for PUSH/POP STATE and PUSH/POP PROJ.

use crate::registers::RegisterFile;
use crate::tier1::{BankId, Mat4Bank};

/// Full-stack: saves/restores the entire RegisterFile (PUSH/POP STATE).
#[derive(Debug, Clone)]
pub struct StateStack {
    frames: Vec<RegisterFile>,
}

impl StateStack {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Push a full snapshot of the register file.
    pub fn push(&mut self, regs: &RegisterFile) {
        self.frames.push(regs.clone());
    }

    /// Pop and restore the register file, marking ALL banks dirty.
    pub fn pop(&mut self, regs: &mut RegisterFile) -> bool {
        if let Some(snapshot) = self.frames.pop() {
            *regs = snapshot;
            // Full restore — mark everything dirty for re-upload
            regs.dirty.mat4 = true;
            regs.dirty.vec4 = true;
            regs.dirty.vec3 = true;
            regs.dirty.scalar = true;
            regs.dirty.int = true;
            true
        } else {
            false
        }
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}

impl Default for StateStack {
    fn default() -> Self {
        Self::new()
    }
}

/// Micro-stack: saves/restores ONLY the Mat4 bank (PUSH/POP PROJ).
/// Critical for Test C — only MAT4 dirty flag should be set on pop.
#[derive(Debug, Clone)]
pub struct ProjStack {
    frames: Vec<Mat4Bank>,
}

impl ProjStack {
    pub fn new() -> Self {
        Self { frames: Vec::new() }
    }

    /// Push only the Mat4 bank.
    pub fn push(&mut self, regs: &RegisterFile) {
        self.frames.push(regs.mat4.clone());
    }

    /// Pop and restore only the Mat4 bank, marking ONLY Mat4 dirty.
    pub fn pop(&mut self, regs: &mut RegisterFile) -> bool {
        if let Some(mat4_snapshot) = self.frames.pop() {
            regs.mat4 = mat4_snapshot;
            // Only mark MAT4 dirty — this is the key invariant for Test C
            regs.dirty.clear_all();
            regs.dirty.set(BankId::Mat4);
            true
        } else {
            false
        }
    }

    pub fn depth(&self) -> usize {
        self.frames.len()
    }
}

impl Default for ProjStack {
    fn default() -> Self {
        Self::new()
    }
}
