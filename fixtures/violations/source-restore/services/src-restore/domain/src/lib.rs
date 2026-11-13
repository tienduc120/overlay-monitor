//! Fixture.
#![no_std]

#[derive(Debug, Clone, Copy)]
pub struct TicketId(u64);

impl TicketId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// A ticket. agent:aggregate
#[derive(Debug)]
pub struct Ticket {
    id: TicketId,
}

impl Ticket {
    pub const fn new(id: TicketId) -> Self {
        Self { id }
    }

    pub const fn restore(id: TicketId) -> Self {
        Self { id }
    }

    pub const fn id(&self) -> TicketId {
        self.id
    }
}
