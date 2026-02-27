//! Modified RPN stack language VM (MTSM-RPN-Bincode).

use crate::arena::TripleArena;
use crate::fqa::Fqa;
use crate::ova::Ova;
use std::collections::HashMap;
use std::fmt;

/// RPN opcodes (u8 repr).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RpnOp {
    Nop = 0x00,
    Push32 = 0x01,
    Push64 = 0x02,
    PushFqa = 0x03,
    Dup = 0x04,
    Drop = 0x05,
    Swap = 0x06,
    Add = 0x07,
    Sub = 0x08,
    Mul = 0x09,
    Div = 0x0A,
    Load = 0x0B,
    Store = 0x0C,
    Call = 0x0D,
    Ret = 0x0E,
    Sync = 0x0F,
    MapNew = 0x10,
    MapSet = 0x11,
    MapGet = 0x12,
}

impl RpnOp {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(RpnOp::Nop),
            0x01 => Some(RpnOp::Push32),
            0x02 => Some(RpnOp::Push64),
            0x03 => Some(RpnOp::PushFqa),
            0x04 => Some(RpnOp::Dup),
            0x05 => Some(RpnOp::Drop),
            0x06 => Some(RpnOp::Swap),
            0x07 => Some(RpnOp::Add),
            0x08 => Some(RpnOp::Sub),
            0x09 => Some(RpnOp::Mul),
            0x0A => Some(RpnOp::Div),
            0x0B => Some(RpnOp::Load),
            0x0C => Some(RpnOp::Store),
            0x0D => Some(RpnOp::Call),
            0x0E => Some(RpnOp::Ret),
            0x0F => Some(RpnOp::Sync),
            0x10 => Some(RpnOp::MapNew),
            0x11 => Some(RpnOp::MapSet),
            0x12 => Some(RpnOp::MapGet),
            _ => None,
        }
    }
}

/// Values on the RPN stack.
#[derive(Debug, Clone)]
pub enum RpnValue {
    U32(u32),
    U64(u64),
    Fqa(Fqa),
    Ova(Ova),
    Map(HashMap<u64, RpnValue>),
}

impl RpnValue {
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            RpnValue::U32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            RpnValue::U64(v) => Some(*v),
            RpnValue::U32(v) => Some(*v as u64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpnError {
    StackUnderflow,
    StackOverflow,
    TypeMismatch,
    DivisionByZero,
    CycleLimitExceeded,
    InvalidOpcode(u8),
    TruncatedBytecode,
    ArenaError(String),
    CallStackUnderflow,
}

impl fmt::Display for RpnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpnError::StackUnderflow => write!(f, "RPN stack underflow"),
            RpnError::StackOverflow => write!(f, "RPN stack overflow"),
            RpnError::TypeMismatch => write!(f, "RPN type mismatch"),
            RpnError::DivisionByZero => write!(f, "RPN division by zero"),
            RpnError::InvalidOpcode(op) => write!(f, "RPN invalid opcode: {:#04x}", op),
            RpnError::TruncatedBytecode => write!(f, "RPN truncated bytecode"),
            RpnError::ArenaError(e) => write!(f, "RPN arena error: {}", e),
            RpnError::CallStackUnderflow => write!(f, "RPN call stack underflow"),
            RpnError::CycleLimitExceeded => write!(f, "RPN cycle limit exceeded"),
        }
    }
}

/// The RPN stack machine.
pub struct RpnVm {
    pub stack: Vec<RpnValue>,
    pub call_stack: Vec<usize>,
    pub pc: usize,
    pub max_stack_depth: usize,
    pub max_cycles: usize,
    pub synced: bool,
}

impl RpnVm {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            call_stack: Vec::new(),
            pc: 0,
            max_stack_depth: 256,
            max_cycles: 1_000_000,
            synced: false,
        }
    }

    fn push(&mut self, val: RpnValue) -> Result<(), RpnError> {
        if self.stack.len() >= self.max_stack_depth {
            return Err(RpnError::StackOverflow);
        }
        self.stack.push(val);
        Ok(())
    }

    fn pop(&mut self) -> Result<RpnValue, RpnError> {
        self.stack.pop().ok_or(RpnError::StackUnderflow)
    }

    fn pop_u64(&mut self) -> Result<u64, RpnError> {
        let v = self.pop()?;
        v.as_u64().ok_or(RpnError::TypeMismatch)
    }

    fn read_u8(bytecode: &[u8], pos: usize) -> Result<u8, RpnError> {
        bytecode.get(pos).copied().ok_or(RpnError::TruncatedBytecode)
    }

    fn read_u32(bytecode: &[u8], pos: usize) -> Result<u32, RpnError> {
        if pos + 4 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u32::from_le_bytes(bytecode[pos..pos + 4].try_into().unwrap()))
    }

    fn read_u64(bytecode: &[u8], pos: usize) -> Result<u64, RpnError> {
        if pos + 8 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u64::from_le_bytes(bytecode[pos..pos + 8].try_into().unwrap()))
    }

    fn read_u128(bytecode: &[u8], pos: usize) -> Result<u128, RpnError> {
        if pos + 16 > bytecode.len() {
            return Err(RpnError::TruncatedBytecode);
        }
        Ok(u128::from_le_bytes(bytecode[pos..pos + 16].try_into().unwrap()))
    }

    /// Execute bytecode against arenas. Returns when bytecode is exhausted or Ret with empty call stack.
    pub fn execute(
        &mut self,
        bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        self.pc = 0;
        self.synced = false;
        let mut cycles = 0usize;

        while self.pc < bytecode.len() {
            cycles += 1;
            if cycles > self.max_cycles {
                return Err(RpnError::CycleLimitExceeded);
            }
            self.step(bytecode, arenas)?;
        }

        Ok(())
    }

    /// Decode and execute a single instruction.
    pub fn step(
        &mut self,
        bytecode: &[u8],
        arenas: &mut TripleArena,
    ) -> Result<(), RpnError> {
        let op_byte = Self::read_u8(bytecode, self.pc)?;
        let op = RpnOp::from_u8(op_byte).ok_or(RpnError::InvalidOpcode(op_byte))?;

        match op {
            RpnOp::Nop => {
                self.pc += 1;
            }
            RpnOp::Push32 => {
                let val = Self::read_u32(bytecode, self.pc + 1)?;
                self.push(RpnValue::U32(val))?;
                self.pc += 5;
            }
            RpnOp::Push64 => {
                let val = Self::read_u64(bytecode, self.pc + 1)?;
                self.push(RpnValue::U64(val))?;
                self.pc += 9;
            }
            RpnOp::PushFqa => {
                let val = Self::read_u128(bytecode, self.pc + 1)?;
                self.push(RpnValue::Fqa(Fqa::new(val)))?;
                self.pc += 17;
            }
            RpnOp::Dup => {
                let val = self.pop()?;
                self.push(val.clone())?;
                self.push(val)?;
                self.pc += 1;
            }
            RpnOp::Drop => {
                self.pop()?;
                self.pc += 1;
            }
            RpnOp::Swap => {
                let b = self.pop()?;
                let a = self.pop()?;
                self.push(b)?;
                self.push(a)?;
                self.pc += 1;
            }
            RpnOp::Add => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_add(b)))?;
                self.pc += 1;
            }
            RpnOp::Sub => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_sub(b)))?;
                self.pc += 1;
            }
            RpnOp::Mul => {
                let b = self.pop_u64()?;
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a.wrapping_mul(b)))?;
                self.pc += 1;
            }
            RpnOp::Div => {
                let b = self.pop_u64()?;
                if b == 0 {
                    return Err(RpnError::DivisionByZero);
                }
                let a = self.pop_u64()?;
                self.push(RpnValue::U64(a / b))?;
                self.pc += 1;
            }
            RpnOp::Load => {
                let ova_val = self.pop()?;
                let ova = match ova_val {
                    RpnValue::Ova(o) => o,
                    RpnValue::U32(v) => Ova(v),
                    _ => return Err(RpnError::TypeMismatch),
                };
                let data = arenas
                    .read(ova)
                    .map_err(|e| RpnError::ArenaError(e.to_string()))?;
                // Load first 4 bytes as u32
                if data.len() >= 4 {
                    let val = u32::from_le_bytes(data[..4].try_into().unwrap());
                    self.push(RpnValue::U32(val))?;
                } else {
                    self.push(RpnValue::U32(0))?;
                }
                self.pc += 1;
            }
            RpnOp::Store => {
                let ova_val = self.pop()?;
                let ova = match ova_val {
                    RpnValue::Ova(o) => o,
                    RpnValue::U32(v) => Ova(v),
                    _ => return Err(RpnError::TypeMismatch),
                };
                let val = self.pop_u64()?;
                let bytes = (val as u32).to_le_bytes();
                arenas
                    .write(ova, &bytes)
                    .map_err(|e| RpnError::ArenaError(e.to_string()))?;
                self.pc += 1;
            }
            RpnOp::Call => {
                let target = self.pop_u64()? as usize;
                self.call_stack.push(self.pc + 1);
                self.pc = target;
            }
            RpnOp::Ret => {
                if let Some(return_pc) = self.call_stack.pop() {
                    self.pc = return_pc;
                } else {
                    // Ret with empty call stack -- halt execution
                    self.pc = bytecode.len();
                }
            }
            RpnOp::Sync => {
                arenas.sync();
                self.synced = true;
                self.pc += 1;
            }
            RpnOp::MapNew => {
                self.push(RpnValue::Map(HashMap::new()))?;
                self.pc += 1;
            }
            RpnOp::MapSet => {
                let val = self.pop()?;
                let key = self.pop_u64()?;
                let mut map_val = self.pop()?;
                if let RpnValue::Map(ref mut map) = map_val {
                    map.insert(key, val);
                    self.push(map_val)?;
                } else {
                    return Err(RpnError::TypeMismatch);
                }
                self.pc += 1;
            }
            RpnOp::MapGet => {
                let key = self.pop_u64()?;
                let map_val = self.pop()?;
                if let RpnValue::Map(ref map) = map_val {
                    if let Some(val) = map.get(&key) {
                        self.push(val.clone())?;
                    } else {
                        self.push(RpnValue::U32(0))?;
                    }
                } else {
                    return Err(RpnError::TypeMismatch);
                }
                self.pc += 1;
            }
        }

        Ok(())
    }

    /// Encode a sequence of instructions to bytecode.
    pub fn encode(instructions: &[(RpnOp, Option<&[u8]>)]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (op, payload) in instructions {
            buf.push(*op as u8);
            if let Some(data) = payload {
                buf.extend_from_slice(data);
            }
        }
        buf
    }

    /// Decode bytecode to instruction list (for debugging).
    pub fn decode(bytecode: &[u8]) -> Result<Vec<(RpnOp, Vec<u8>)>, RpnError> {
        let mut result = Vec::new();
        let mut pc = 0;

        while pc < bytecode.len() {
            let op_byte = bytecode[pc];
            let op = RpnOp::from_u8(op_byte).ok_or(RpnError::InvalidOpcode(op_byte))?;
            let payload_size = match op {
                RpnOp::Push32 => 4,
                RpnOp::Push64 => 8,
                RpnOp::PushFqa => 16,
                _ => 0,
            };
            if pc + 1 + payload_size > bytecode.len() {
                return Err(RpnError::TruncatedBytecode);
            }
            let payload = bytecode[pc + 1..pc + 1 + payload_size].to_vec();
            result.push((op, payload));
            pc += 1 + payload_size;
        }

        Ok(result)
    }
}

impl Default for RpnVm {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RpnVm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RpnVm(pc={}, stack_depth={})", self.pc, self.stack.len())
    }
}
