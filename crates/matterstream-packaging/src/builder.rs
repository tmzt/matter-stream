//! PackageBuilder — assembles compiled components into a .mtsm archive.

use matterstream_vm_addressing::fqa::{FourCC, Ordinal, Fqa};
use matterstream_vm_addressing::oid::Oid;
use matterstream_vm_addressing::oid::ImportKind;
use matterstream_vm_addressing::oid_index::OidIndexBuilder;
use crate::archive::{MtsmArchive, ArchiveMember};
use crate::tkv::{TkvEntry, TkvValue};
use crate::loader::{LoadedComponent, encode_stab, encode_ctab};

/// Compiled component output (mirrors AsmOutput from matterstream-vm-asm).
pub struct CompiledComponent {
    pub bytecode: Vec<u8>,
    pub string_table: Vec<String>,
}

/// Builds a .mtsm archive from compiled components.
pub struct PackageBuilder {
    name: String,
    components: Vec<(String, Oid, u128, CompiledComponent)>, // (name, oid, fqa, compiled)
    next_fqa: u128,
}

impl PackageBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            components: Vec::new(),
            // Start FQAs at a deterministic base derived from package name
            next_fqa: {
                let mut h: u128 = 0x0001_0000_0000_0000;
                for b in name.bytes() {
                    h = h.wrapping_mul(31).wrapping_add(b as u128);
                }
                h | 0x0001
            },
        }
    }

    /// Add a compiled component with an explicit FQA.
    pub fn add_with_fqa(&mut self, name: &str, oid: Oid, fqa: u128, compiled: CompiledComponent) {
        self.components.push((name.to_string(), oid, fqa, compiled));
    }

    /// Add a compiled component with an auto-generated FQA.
    pub fn add(&mut self, name: &str, oid: Oid, compiled: CompiledComponent) {
        let fqa = self.next_fqa;
        self.next_fqa += 1;
        self.add_with_fqa(name, oid, fqa, compiled);
    }

    /// Build the .mtsm archive.
    pub fn build(self) -> MtsmArchive {
        let mut archive = MtsmArchive::new();
        let mut ordinal_counter = 0u64;

        // .meta manifest (ordinal 0)
        let manifest = crate::tkv::TkvDocument { entries: vec![
            TkvEntry { key: "name".to_string(), value: TkvValue::String(self.name.clone()) },
            TkvEntry { key: "version".to_string(), value: TkvValue::String("0.1.0".to_string()) },
            TkvEntry { key: "component_count".to_string(), value: TkvValue::Integer(self.components.len() as u64) },
        ] };
        archive.add(ArchiveMember {
            ordinal: Ordinal::from_u64(ordinal_counter),
            fourcc: FourCC::Meta,
            data: manifest.encode(),
        });
        ordinal_counter += 1;

        // .asym placeholder (ordinal 1)
        archive.add(ArchiveMember {
            ordinal: Ordinal::from_u64(ordinal_counter),
            fourcc: FourCC::Asym,
            data: vec![0u8; 8],
        });
        ordinal_counter += 1;

        // Concatenate bytecodes and merge string tables
        let mut all_bytecode = Vec::new();
        let mut all_strings = Vec::new();
        let mut loaded_components = Vec::new();
        let mut oid_builder = OidIndexBuilder::new();

        for (_, oid, fqa, compiled) in &self.components {
            let offset = all_bytecode.len() as u32;
            let length = compiled.bytecode.len() as u32;
            let string_base = all_strings.len() as u32;

            all_bytecode.extend_from_slice(&compiled.bytecode);
            all_strings.extend_from_slice(&compiled.string_table);

            loaded_components.push(LoadedComponent {
                fqa: *fqa,
                offset,
                length,
                string_base,
            });

            // Register in OID index
            let fqa_addr = Fqa::new(*fqa);
            oid_builder.add_fqa(*oid, ImportKind::Component, fqa_addr);
        }

        // .mrbc (ordinal 2)
        archive.add(ArchiveMember {
            ordinal: Ordinal::from_u64(ordinal_counter),
            fourcc: FourCC::Mrbc,
            data: all_bytecode,
        });
        ordinal_counter += 1;

        // .osym (ordinal 3)
        archive.add(ArchiveMember {
            ordinal: Ordinal::from_u64(ordinal_counter),
            fourcc: FourCC::Osym,
            data: oid_builder.build(),
        });
        ordinal_counter += 1;

        // .ctab (ordinal 4)
        archive.add(ArchiveMember {
            ordinal: Ordinal::from_u64(ordinal_counter),
            fourcc: FourCC::Ctab,
            data: encode_ctab(&loaded_components),
        });
        ordinal_counter += 1;

        // .stab (ordinal 5)
        archive.add(ArchiveMember {
            ordinal: Ordinal::from_u64(ordinal_counter),
            fourcc: FourCC::Stab,
            data: encode_stab(&all_strings),
        });

        archive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load_package;

    #[test]
    fn builder_roundtrip() {
        let comp_a = CompiledComponent {
            bytecode: vec![0x00, 0x0F], // Nop, Halt
            string_table: vec!["hello".to_string()],
        };
        let comp_b = CompiledComponent {
            bytecode: vec![0x00, 0x00, 0x0F], // Nop, Nop, Halt
            string_table: vec!["world".to_string(), "!".to_string()],
        };

        let oid_a = Oid::new(0, 0x01);
        let oid_b = Oid::new(0, 0x02);

        let mut builder = PackageBuilder::new("test-pkg");
        builder.add_with_fqa("CompA", oid_a, 0xAAAA, comp_a);
        builder.add_with_fqa("CompB", oid_b, 0xBBBB, comp_b);
        let archive = builder.build();

        // Serialize and parse back
        let bytes = archive.to_ar_bytes();
        let parsed = MtsmArchive::from_ar_bytes(&bytes).unwrap();

        let loaded = load_package(&parsed).unwrap();
        assert_eq!(loaded.bytecode.len(), 5); // 2 + 3
        assert_eq!(loaded.strings, vec!["hello", "world", "!"]);
        assert_eq!(loaded.components.len(), 2);

        let ca = &loaded.components[0];
        assert_eq!(ca.fqa, 0xAAAA);
        assert_eq!(ca.offset, 0);
        assert_eq!(ca.length, 2);
        assert_eq!(ca.string_base, 0);

        let cb = &loaded.components[1];
        assert_eq!(cb.fqa, 0xBBBB);
        assert_eq!(cb.offset, 2);
        assert_eq!(cb.length, 3);
        assert_eq!(cb.string_base, 1);

        assert!(loaded.osym.is_some());
    }
}
