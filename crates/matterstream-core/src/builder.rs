//! A builder API for constructing `Op` sequences.

use crate::ops::{Op, Primitive};
use crate::tier3::ResourceHandle;

/// A builder for constructing `Op` sequences.
pub struct StreamBuilder {
    ops: Vec<Op>,
}

impl StreamBuilder {
    /// Creates a new `StreamBuilder`.
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    /// Adds a `Draw` op.
    pub fn draw(mut self, primitive: Primitive, position_rsi: usize) -> Self {
        self.ops.push(Op::Draw {
            primitive,
            position_rsi,
        });
        self
    }

    /// Adds a `SetTrans` op.
    pub fn set_trans(mut self, translation: [f32; 3]) -> Self {
        self.ops.push(Op::SetTrans(translation));
        self
    }

    /// Adds a `SetColor` op.
    pub fn set_color(mut self, color: [f32; 4]) -> Self {
        self.ops.push(Op::SetColor(color));
        self
    }

    /// Adds a `SetSize` op.
    pub fn set_size(mut self, size: [f32; 2]) -> Self {
        self.ops.push(Op::SetSize(size));
        self
    }

    /// Adds a `SetLabel` op.
    pub fn set_label(mut self, text: impl Into<String>) -> Self {
        self.ops.push(Op::SetLabel(text.into()));
        self
    }

    /// Adds a `SetPadding` op.
    pub fn set_padding(mut self, padding: [f32; 4]) -> Self {
        self.ops.push(Op::SetPadding(padding));
        self
    }

    /// Adds a `SetTextColor` op.
    pub fn set_text_color(mut self, color: [f32; 4]) -> Self {
        self.ops.push(Op::SetTextColor(color));
        self
    }

    /// Adds a `SetMatrix` op.
    pub fn set_matrix(mut self, matrix: [f32; 16]) -> Self {
        self.ops.push(Op::SetMatrix(matrix));
        self
    }

    /// Adds a `PushProj` op.
    pub fn push_proj(mut self) -> Self {
        self.ops.push(Op::PushProj);
        self
    }

    /// Adds a `PopProj` op.
    pub fn pop_proj(mut self) -> Self {
        self.ops.push(Op::PopProj);
        self
    }

    /// Adds a `PushState` op.
    pub fn push_state(mut self) -> Self {
        self.ops.push(Op::PushState);
        self
    }

    /// Adds a `PopState` op.
    pub fn pop_state(mut self) -> Self {
        self.ops.push(Op::PopState);
        self
    }

    /// Adds a `BindZeroPage` op.
    pub fn bind_zero_page(mut self, offset: u8, len: u8) -> Self {
        self.ops.push(Op::BindZeroPage { offset, len });
        self
    }

    /// Adds a `BindResource` op.
    pub fn bind_resource(mut self, handle: ResourceHandle) -> Self {
        self.ops.push(Op::BindResource(handle));
        self
    }

    /// Adds a `Push` op.
    pub fn push(mut self, data: Vec<u8>) -> Self {
        self.ops.push(Op::Push(data));
        self
    }

    /// Builds the `Op` sequence.
    pub fn build(self) -> Vec<Op> {
        self.ops
    }
}

impl Default for StreamBuilder {
    fn default() -> Self {
        Self::new()
    }
}
