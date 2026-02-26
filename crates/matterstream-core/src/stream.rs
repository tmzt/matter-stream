//! MatterStream executor — hydrates ops, drives execution lifecycle.

use crate::ops::{Draw, Op, OpsHeader, Primitive};
use crate::registers::RegisterFile;
use crate::state_stack::{ProjStack, StateStack};
use crate::tier0::GlobalUniforms;
use crate::tier1::{BankId, Mat4Bank, Vec3Bank};
use crate::tier2::ZeroPage;
use crate::tier3::ResourceTable;

#[derive(Debug)]
pub enum StreamError {
    InvalidRsi(usize),
}

/// The main stream executor: owns all tiers, register file, and state stacks.
pub struct MatterStream {
    pub globals: GlobalUniforms,
    pub registers: RegisterFile,
    pub zero_page: ZeroPage,
    pub resources: ResourceTable,
    pub state_stack: StateStack,
    pub proj_stack: ProjStack,
    /// Collected draw results from the last execution.
    pub draws: Vec<Draw>,
    /// The raw byte stream.
    pub stream: Vec<u8>,
    /// Pending label for the next draw call (consumed on draw).
    pending_label: Option<String>,
    /// Pending padding for the next draw call (consumed on draw).
    pending_padding: [f32; 4],
    /// Pending text color for the next draw call (consumed on draw).
    pending_text_color: Option<[f32; 4]>,
}

impl MatterStream {
    pub fn new() -> Self {
        Self {
            globals: GlobalUniforms::new(),
            registers: RegisterFile::new(),
            zero_page: ZeroPage::new(),
            resources: ResourceTable::new(),
            state_stack: StateStack::new(),
            proj_stack: ProjStack::new(),
            draws: Vec::new(),
            stream: Vec::new(),
            pending_label: None,
            pending_padding: [0.0; 4],
            pending_text_color: None,
        }
    }

    /// Execute a sequence of ops with the given header.
    pub async fn execute(&mut self, header: &OpsHeader, ops: &[Op]) -> Result<(), Vec<StreamError>> {
        self.draws.clear();
        self.stream.clear();
        let mut errors = Vec::new();

        // Step 1: Hydrate RSI pointers into registers
        self.registers
            .hydrate(&header.rsi_pointers, &self.zero_page, &self.resources)
            .await;

        // Step 2: Process each op
        for op in ops {
            match op {
                Op::Draw {
                    primitive,
                    position_rsi,
                } => {
                    let label = self.pending_label.take();
                    if let Some(result) = self.execute_draw(header, primitive, *position_rsi, label) {
                        self.draws.push(result);
                    } else {
                        errors.push(StreamError::InvalidRsi(*position_rsi));
                    }
                }
                Op::SetTrans(translation) => {
                    // Translation fast-path: write vec3, mark Vec3 dirty
                    self.registers.vec3.write(0, *translation);
                    self.registers.dirty.set(BankId::Vec3);
                }
                Op::SetMatrix(matrix) => {
                    self.registers.mat4.write(0, *matrix);
                    self.registers.dirty.set(BankId::Mat4);
                }
                Op::SetColor(color) => {
                    self.registers.vec4.write(0, *color);
                    self.registers.dirty.set(BankId::Vec4);
                }
                Op::SetSize(size) => {
                    // Store size in Vec3 bank register 1: [w, h, 0.0]
                    self.registers.vec3.write(1, [size[0], size[1], 0.0]);
                    self.registers.dirty.set(BankId::Vec3);
                }
                Op::PushProj => {
                    self.proj_stack.push(&self.registers);
                }
                Op::PopProj => {
                    self.proj_stack.pop(&mut self.registers);
                }
                Op::PushState => {
                    self.state_stack.push(&self.registers);
                }
                Op::PopState => {
                    self.state_stack.pop(&mut self.registers);
                }
                Op::SetLabel(text) => {
                    self.pending_label = Some(text.clone());
                }
                Op::SetPadding(p) => {
                    self.pending_padding = *p;
                }
                Op::SetTextColor(c) => {
                    self.pending_text_color = Some(*c);
                }
                Op::BindZeroPage { .. } | Op::BindResource(_) => {
                    // Binding ops update execution context
                }
                Op::Push(data) => {
                    self.stream.extend_from_slice(data);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Execute a draw, resolving position via direct register index.
    fn execute_draw(&mut self, header: &OpsHeader, primitive: &Primitive, position_rsi: usize, label: Option<String>) -> Option<Draw> {
        // Resolve position from the RSI pointer — O(1) direct register access (Test A)
        let rsi = header.rsi_pointers.get(position_rsi)?;

        let position = if rsi.tier == 1 {
            // Direct register access
            let bank = BankId::from_u8(rsi.bank).unwrap_or(BankId::Vec3);
            self.registers.read_position(bank, rsi.index as usize)
        } else {
            [0.0; 3]
        };

        let color = *self.registers.vec4.read(0);

        // Read size from Vec3 bank register 1 (set by Op::SetSize), then reset
        let size_reg = self.registers.vec3.read(1);
        let size = [size_reg[0], size_reg[1]];
        self.registers.vec3.write(1, [0.0, 0.0, 0.0]);

        let padding = std::mem::replace(&mut self.pending_padding, [0.0; 4]);
        let text_color = self.pending_text_color.take();

        // Translation fast-path check (Test D)
        if header.translation_only {
            // 12 bytes: vec3 addition
            Some(Draw {
                primitive: primitive.clone(),
                position,
                color,
                size,
                label,
                padding,
                text_color,
                used_fast_path: true,
                transform_bytes: Vec3Bank::register_bytes(),
            })
        } else {
            // 64 bytes: full mat4 multiplication
            Some(Draw {
                primitive: primitive.clone(),
                position,
                color,
                size,
                label,
                padding,
                text_color,
                used_fast_path: false,
                transform_bytes: Mat4Bank::register_bytes(),
            })
        }
    }
}

impl Default for MatterStream {
    fn default() -> Self {
        Self::new()
    }
}
