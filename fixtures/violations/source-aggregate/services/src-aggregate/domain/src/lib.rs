//! Fixture: the id is a bare String.
#![no_std]

extern crate alloc;

use alloc::string::String;

/// An order. agent:aggregate
pub struct Order {
    id: String,
}

impl Order {
    pub const fn new(id: String) -> Self {
        Self { id }
    }

    pub const fn restore(id: String) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}
