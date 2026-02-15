//! Tier 3 — Resource Handles (Extended memory analog)
//!
//! 8-bit type-tagged handles for BBOs, Textures, and Fonts.

/// Known resource type tags.
pub const TYPE_BBO: u8 = 0;
pub const TYPE_TEXTURE: u8 = 1;
pub const TYPE_FONT: u8 = 2;

/// Compact resource handle: 8-bit type tag + 8-bit index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceHandle {
    pub type_tag: u8,
    pub index: u8,
}

impl ResourceHandle {
    pub fn new(type_tag: u8, index: u8) -> Self {
        Self { type_tag, index }
    }

    pub fn bbo(index: u8) -> Self {
        Self::new(TYPE_BBO, index)
    }

    pub fn texture(index: u8) -> Self {
        Self::new(TYPE_TEXTURE, index)
    }

    pub fn font(index: u8) -> Self {
        Self::new(TYPE_FONT, index)
    }
}

/// Descriptor for a bound buffer object.
#[derive(Debug, Clone)]
pub struct BboDescriptor {
    pub data: Vec<u8>,
    pub stride: usize,
}

/// Descriptor for a texture resource.
#[derive(Debug, Clone)]
pub struct TextureDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: u32,
}

/// Descriptor for a font resource.
#[derive(Debug, Clone)]
pub struct FontDescriptor {
    pub name: String,
    pub size: f32,
}

/// Resource variant stored in the table.
#[derive(Debug, Clone)]
pub enum Resource {
    Bbo(BboDescriptor),
    Texture(TextureDescriptor),
    Font(FontDescriptor),
}

/// Table mapping handles to resource descriptors.
#[derive(Debug, Clone)]
pub struct ResourceTable {
    entries: Vec<Option<Resource>>,
}

impl ResourceTable {
    pub fn new() -> Self {
        Self {
            entries: vec![None; 256],
        }
    }

    /// Insert a resource, returning a handle.
    pub fn insert(&mut self, type_tag: u8, resource: Resource) -> ResourceHandle {
        // Find next free slot
        for (i, slot) in self.entries.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(resource);
                return ResourceHandle::new(type_tag, i as u8);
            }
        }
        panic!("resource table full");
    }

    /// Look up a resource by handle.
    pub fn get(&self, handle: ResourceHandle) -> Option<&Resource> {
        self.entries.get(handle.index as usize).and_then(|s| s.as_ref())
    }

    /// Stride-based indexing into a BBO array.
    pub fn bbo_element(&self, handle: ResourceHandle, element: usize) -> Option<&[u8]> {
        match self.get(handle)? {
            Resource::Bbo(bbo) => {
                let start = element * bbo.stride;
                if start + bbo.stride <= bbo.data.len() {
                    Some(&bbo.data[start..start + bbo.stride])
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Default for ResourceTable {
    fn default() -> Self {
        Self::new()
    }
}
