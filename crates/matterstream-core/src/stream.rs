//! MatterStream executor — hydrates ops, drives execution lifecycle.

use matterstream_vm_addressing::addressing::AddressResolver;
use matterstream_vm_arena::arena::TripleArena;
use matterstream_vm_arena::dmove::DmoveEngine;
use matterstream_vm_scl::keyless::KeylessPolicy;
use crate::ops::{Draw, Op, OpsHeader, Primitive};
use crate::registers::RegisterFile;
use crate::rpn::RpnVm;
use crate::state_stack::{ProjStack, StateStack};
use crate::tier0::GlobalUniforms;
use crate::tier1::{BankId, Mat4Bank, Vec3Bank};
use crate::tier2::ZeroPage;
use crate::tier3::ResourceTable;
use crate::ui_vm::UiDrawCmd;

#[derive(Debug)]
pub enum StreamError {
    InvalidRsi(usize),
    ResolveError(String),
    ArenaError(String),
    RpnError(String),
    DmoveError(String),
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

    /// UI draw commands collected from RPN VM execution.
    pub ui_draws: Vec<UiDrawCmd>,

    // VM_SPEC v0.1.0 fields
    pub arenas: TripleArena,
    pub resolver: AddressResolver,
    pub rpn_vm: RpnVm,
    pub keyless: KeylessPolicy,
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
            ui_draws: Vec::new(),
            arenas: TripleArena::new(),
            resolver: AddressResolver::new(),
            rpn_vm: RpnVm::new(),
            keyless: KeylessPolicy::new(),
        }
    }

    /// Execute a sequence of ops with the given header.
    pub async fn execute(&mut self, header: &OpsHeader, ops: &[Op]) -> Result<(), Vec<StreamError>> {
        self.draws.clear();
        self.stream.clear();
        self.ui_draws.clear();
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
                    if let Some(result) = self.execute_draw(header, primitive, *position_rsi) {
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
                Op::BindZeroPage { .. } | Op::BindResource(_) => {
                    // Binding ops update execution context
                }
                Op::Push(data) => {
                    self.stream.extend_from_slice(data);
                }
                Op::ResolveFqa(fqa) => {
                    match self.resolver.resolve(*fqa) {
                        Ok(_ova) => { /* resolved successfully */ }
                        Err(e) => errors.push(StreamError::ResolveError(e.to_string())),
                    }
                }
                Op::Sync => {
                    self.arenas.sync();
                }
                Op::ExecRpn(bytecode) => {
                    if let Err(e) = self.rpn_vm.execute(bytecode, &mut self.arenas) {
                        errors.push(StreamError::RpnError(e.to_string()));
                    }
                    self.ui_draws.append(&mut self.rpn_vm.ui_draws);
                }
                Op::Dmove(descriptors) => {
                    if let Err(e) = DmoveEngine::execute(&mut self.arenas, descriptors) {
                        errors.push(StreamError::DmoveError(e.to_string()));
                    }
                }
                Op::LoadArchiveMember { .. } => {
                    // Archive loading handled at a higher level
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
    fn execute_draw(&self, header: &OpsHeader, _primitive: &Primitive, position_rsi: usize) -> Option<Draw> {
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

        // Translation fast-path check (Test D)
        if header.translation_only {
            // 12 bytes: vec3 addition
            Some(Draw {
                position,
                color,
                used_fast_path: true,
                transform_bytes: Vec3Bank::register_bytes(),
            })
        } else {
            // 64 bytes: full mat4 multiplication
            Some(Draw {
                position,
                color,
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
