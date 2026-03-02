//! Full address resolution: FQA -> Ordinal -> ASLR Token -> OVA.

use crate::aslr::{AslrToken, AsymTable};
use crate::fqa::Fqa;
use crate::ova::Ova;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    UnknownFqa(Fqa),
    TokenNotFound(AslrToken),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::UnknownFqa(fqa) => write!(f, "unknown FQA: {}", fqa),
            ResolveError::TokenNotFound(tok) => write!(f, "ASLR token not found: {}", tok),
        }
    }
}

/// Full 4-stage address resolver: FQA -> token -> OVA.
pub struct AddressResolver {
    fqa_to_token: HashMap<Fqa, AslrToken>,
    asym: AsymTable,
}

impl AddressResolver {
    pub fn new() -> Self {
        Self {
            fqa_to_token: HashMap::new(),
            asym: AsymTable::new(),
        }
    }

    /// Register an FQA -> ASLR token -> OVA mapping.
    pub fn register(&mut self, fqa: Fqa, token: AslrToken, ova: Ova) {
        self.fqa_to_token.insert(fqa, token);
        self.asym.insert(token, ova);
    }

    /// Full resolution: FQA -> ASLR token -> OVA.
    pub fn resolve(&self, fqa: Fqa) -> Result<Ova, ResolveError> {
        let token = self
            .fqa_to_token
            .get(&fqa)
            .copied()
            .ok_or(ResolveError::UnknownFqa(fqa))?;
        self.asym
            .resolve(token)
            .ok_or(ResolveError::TokenNotFound(token))
    }

    /// Replace the ASYM table (e.g., after loading from archive).
    pub fn swap_asym(&mut self, new_asym: AsymTable) {
        self.asym = new_asym;
    }

    pub fn asym(&self) -> &AsymTable {
        &self.asym
    }

    pub fn asym_mut(&mut self) -> &mut AsymTable {
        &mut self.asym
    }
}

impl Default for AddressResolver {
    fn default() -> Self {
        Self::new()
    }
}
