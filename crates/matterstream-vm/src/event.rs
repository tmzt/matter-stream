//! Event types for the VM event queue.

/// VM event type codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum VmEventType {
    None = 0,
    KeyDown = 1,
    KeyUp = 2,
    MouseDown = 3,
    MouseUp = 4,
    MouseMove = 5,
    Tick = 6,
}

impl VmEventType {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(VmEventType::None),
            1 => Some(VmEventType::KeyDown),
            2 => Some(VmEventType::KeyUp),
            3 => Some(VmEventType::MouseDown),
            4 => Some(VmEventType::MouseUp),
            5 => Some(VmEventType::MouseMove),
            6 => Some(VmEventType::Tick),
            _ => None,
        }
    }
}

/// A single event in the VM event queue.
#[derive(Debug, Clone, Copy)]
pub struct VmEvent {
    /// Event type.
    pub etype: VmEventType,
    /// Packed data: key code, or (x << 32 | y), or dt_ms for Tick.
    pub data: u64,
}

impl VmEvent {
    pub fn key_down(key_code: u32) -> Self {
        Self {
            etype: VmEventType::KeyDown,
            data: key_code as u64,
        }
    }

    pub fn key_up(key_code: u32) -> Self {
        Self {
            etype: VmEventType::KeyUp,
            data: key_code as u64,
        }
    }

    pub fn mouse_down(x: u32, y: u32) -> Self {
        Self {
            etype: VmEventType::MouseDown,
            data: ((x as u64) << 32) | y as u64,
        }
    }

    pub fn mouse_up(x: u32, y: u32) -> Self {
        Self {
            etype: VmEventType::MouseUp,
            data: ((x as u64) << 32) | y as u64,
        }
    }

    pub fn mouse_move(x: u32, y: u32) -> Self {
        Self {
            etype: VmEventType::MouseMove,
            data: ((x as u64) << 32) | y as u64,
        }
    }

    pub fn tick(dt_ms: u32) -> Self {
        Self {
            etype: VmEventType::Tick,
            data: dt_ms as u64,
        }
    }

    /// Extract mouse x from packed data.
    pub fn mouse_x(&self) -> u32 {
        (self.data >> 32) as u32
    }

    /// Extract mouse y from packed data.
    pub fn mouse_y(&self) -> u32 {
        self.data as u32
    }

    /// Extract key code from packed data.
    pub fn key_code(&self) -> u32 {
        self.data as u32
    }
}

/// Well-known key codes matching common virtual key codes.
pub mod keys {
    pub const ARROW_LEFT: u32 = 37;
    pub const ARROW_UP: u32 = 38;
    pub const ARROW_RIGHT: u32 = 39;
    pub const ARROW_DOWN: u32 = 40;
    pub const SPACE: u32 = 32;
    pub const ENTER: u32 = 13;
    pub const ESCAPE: u32 = 27;
}
