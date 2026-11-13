//! Clean twin: an aggregate in a `no_std` domain, built by `new` and rebuilt by `restore`.
#![no_std]

extern crate alloc;

use alloc::string::String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceId(String);

impl InvoiceId {
    pub fn new(raw: &str) -> Option<Self> {
        (!raw.is_empty()).then(|| Self(String::from(raw)))
    }
}

/// An invoice. agent:aggregate
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoice {
    id: InvoiceId,
    total_cents: u64,
}

impl Invoice {
    pub fn new(id: InvoiceId, total_cents: u64) -> Option<Self> {
        (total_cents > 0).then_some(Self { id, total_cents })
    }

    pub const fn restore(id: InvoiceId, total_cents: u64) -> Self {
        Self { id, total_cents }
    }

    pub const fn id(&self) -> &InvoiceId {
        &self.id
    }

    pub const fn total_cents(&self) -> u64 {
        self.total_cents
    }
}

#[cfg(test)]
mod tests;
