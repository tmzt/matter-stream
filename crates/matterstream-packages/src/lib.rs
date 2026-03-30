// In crates/matterstream-packages/src/lib.rs

use matterstream_core::{MtsmVariant, MtsmExecFunctionalComponent, TsxElementContext, TsxFragment};
use dashmap::DashMap;

pub trait ImportablePackage: 'static + Send + Sync {
    /// Returns the namespace of the package (e.g., "@mtsm/ui/core").
    fn namespace(&self) -> &str;

    /// Resolves a component/primitive by its name within this package.
    fn resolve_component(&self, name: &str) -> Option<Box<dyn MtsmExecFunctionalComponent>>;
}

pub struct PackageRegistry {
    packages: DashMap<String, Box<dyn ImportablePackage>>,
}

impl PackageRegistry {
    pub fn new() -> Self {
        Self {
            packages: DashMap::new(),
        }
    }

    pub fn register_package<P: ImportablePackage + 'static>(&mut self, package: P) {
        self.packages.insert(package.namespace().to_string(), Box::new(package));
    }

    /// Resolves a component given a full import path (e.g., "@mtsm/ui/core/Slab").
    pub fn resolve_full_import_path(&self, import_path: &str) -> Option<Box<dyn MtsmExecFunctionalComponent>> {
        let parts: Vec<&str> = import_path.split('/').collect();
        if parts.len() < 2 {
            return None;
        }

        let component_name = parts.last()?;
        let namespace = parts[0..parts.len()-1].join("/");

        self.packages.get(&namespace)
            .and_then(|package| package.resolve_component(component_name))
    }
}

// Implement core's MtsmPackageRegistry trait for this PackageRegistry
impl matterstream_core::MtsmPackageRegistry for PackageRegistry {
    fn get_namespace_handle(&self, namespace: &str) -> Option<matterstream_core::MtsmPackageHandle> {
        // Simple deterministic handle: hash the namespace string to u64
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        namespace.hash(&mut hasher);
        Some(hasher.finish())
    }

    fn resolve_full_import_path(&self, import_path: &str) -> Option<Box<dyn matterstream_core::MtsmExecFunctionalComponent>> {
        self.resolve_full_import_path(import_path)
    }
}
pub struct SlabPrimitive;

impl MtsmExecFunctionalComponent for SlabPrimitive {
    fn execute(&self, context: TsxElementContext) -> TsxFragment {
        // For now, this is a placeholder. In a real scenario, it would
        // interpret the context (attributes and children) and generate
        // the actual TsxFragment representation of a Slab.
        // It might also generate actual rendering Ops.
        eprintln!("Executing SlabPrimitive with context: {:?}", context.attributes.attributes);
        TsxFragment { elements: Vec::new() }
    }
}

pub struct CoreUiPackage;

impl ImportablePackage for CoreUiPackage {
    fn namespace(&self) -> &str {
        "@mtsm/ui/core"
    }

    fn resolve_component(&self, name: &str) -> Option<Box<dyn MtsmExecFunctionalComponent>> {
        match name {
            "Slab" => Some(Box::new(SlabPrimitive)),
            _ => None,
        }
    }
}
