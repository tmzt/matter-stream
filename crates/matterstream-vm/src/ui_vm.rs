//! UI types, VQL, Skill, ObjectType, and Card definitions.
//!
//! UI draw types (`UiDrawCmd`, `UiDrawState`, transforms, rasterizer) live in
//! `matterstream-ui` and are re-exported here when the `ui` feature is enabled.
//! Without `ui`, UI opcodes in the VM are NOPs.

// ── Re-exports from matterstream-ui (feature-gated) ────────────────────

#[cfg(feature = "ui")]
pub use matterstream_ui::*;

// ── Constants (always available) ────────────────────────────────────────

/// Maximum VQL outputs per execution.
pub const VQL_OUTPUT_MAX: usize = 256;

/// Maximum SKLL outputs per execution.
pub const SKILL_OUTPUT_MAX: usize = 64;

/// Maximum object type definitions per execution.
pub const OBJECT_TYPE_MAX: usize = 64;

/// Maximum card definitions per execution.
pub const CARD_DEF_MAX: usize = 64;

// ── Control Register constants ──────────────────────────────────────────

/// Control register index for output mode FourCC.
pub const CR_OUTPUT_MODE: usize = 0;

/// FourCC: MatterStream UI output (default).
pub const FOURCC_MTUI: u32 = 0x4D545549;
/// FourCC: Vesicle Query Language.
pub const FOURCC_VQL0: u32 = 0x56514C30;
/// FourCC: Skill / invocable logic.
pub const FOURCC_SKLL: u32 = 0x534B4C4C;
/// FourCC: Skill execution / host callbacks.
pub const FOURCC_SKLS: u32 = 0x534B4C53;
/// FourCC: MTD1 document/text layout.
pub const FOURCC_MTD1: u32 = 0x4D544431;

// ── VQL (Vesicle Query Language) types ──────────────────────────────────

/// A single field in a VQL query output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VqlField {
    /// A projected field name.
    Project(String),
    /// A filter predicate name.
    Filter(String),
    /// A bound parameter (name, value from string table).
    Bind { name: String, value: String },
    /// A named parameter (key, value).
    Param { key: String, value: String },
    /// A field with a numeric value.
    FieldValue { name: String, value: u64 },
    /// A field with a string value.
    FieldStr { name: String, value: String },
}

/// A complete VQL query output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VqlOutput {
    pub fields: Vec<VqlField>,
}

impl VqlOutput {
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }
}

impl Default for VqlOutput {
    fn default() -> Self {
        Self::new()
    }
}

// ── SKLL (Skill) types ──────────────────────────────────────────────────

/// LLM use-case hint for routing/dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LlmUseCase {
    General = 0,
    Routing = 1,
    Thinking = 2,
    DeepResearch = 3,
    Summarize = 4,
    CodeGen = 5,
    Extract = 6,
    Validate = 7,
}

impl LlmUseCase {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::General),
            1 => Some(Self::Routing),
            2 => Some(Self::Thinking),
            3 => Some(Self::DeepResearch),
            4 => Some(Self::Summarize),
            5 => Some(Self::CodeGen),
            6 => Some(Self::Extract),
            7 => Some(Self::Validate),
            _ => None,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "general" => Some(Self::General),
            "routing" => Some(Self::Routing),
            "thinking" => Some(Self::Thinking),
            "deep-research" | "deep_research" => Some(Self::DeepResearch),
            "summarize" => Some(Self::Summarize),
            "codegen" | "code-gen" | "code_gen" => Some(Self::CodeGen),
            "extract" => Some(Self::Extract),
            "validate" => Some(Self::Validate),
            _ => None,
        }
    }
}

impl Default for LlmUseCase {
    fn default() -> Self {
        Self::General
    }
}

/// A single step within a skill definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillStep {
    Deterministic { name: String },
    Llm {
        prompt: String,
        replaceables: Vec<SkillReplaceable>,
        model: Option<String>,
        use_case: LlmUseCase,
    },
    InvokeAction { name: String },
    InvokeSymbol { symbol: u32 },
    ForwardPrompt { dest: String },
    AddToSystemPrompt { content: String },
}

/// A replaceable placeholder within an LLM prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillReplaceable {
    pub name: String,
    pub default: String,
}

// ── Object type definitions ─────────────────────────────────────────────

/// A field definition within an object type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFieldDef {
    pub name: String,
    pub fts: bool,
    pub vec: bool,
}

/// A user-defined object type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTypeDef {
    pub name: String,
    pub short_description: String,
    pub long_description: String,
    pub fields: Vec<ObjectFieldDef>,
}

impl ObjectTypeDef {
    pub fn new(name: String) -> Self {
        Self {
            name,
            short_description: String::new(),
            long_description: String::new(),
            fields: Vec::new(),
        }
    }
}

/// Optional cron schedule attached to a skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSpec {
    pub interval_ms: u64,
    pub jitter_ms: u64,
}

/// A complete skill definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDef {
    pub name: String,
    pub short_description: String,
    pub long_description: String,
    pub steps: Vec<SkillStep>,
    pub cron: Option<CronSpec>,
    pub object_types: Vec<ObjectTypeDef>,
    #[cfg(feature = "ui")]
    pub cards: Vec<CardDef>,
}

impl SkillDef {
    pub fn new(name: String) -> Self {
        Self {
            name,
            short_description: String::new(),
            long_description: String::new(),
            steps: Vec::new(),
            cron: None,
            object_types: Vec::new(),
            #[cfg(feature = "ui")]
            cards: Vec::new(),
        }
    }
}

// ── Card definitions ────────────────────────────────────────────────────

/// A named UI card definition with compiled draw commands.
#[cfg(feature = "ui")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDef {
    pub name: String,
    pub short_description: String,
    pub long_description: String,
    pub draws: Vec<UiDrawCmd>,
    pub string_table: Vec<String>,
}

#[cfg(feature = "ui")]
impl CardDef {
    pub fn new(name: String) -> Self {
        Self {
            name,
            short_description: String::new(),
            long_description: String::new(),
            draws: Vec::new(),
            string_table: Vec::new(),
        }
    }
}
