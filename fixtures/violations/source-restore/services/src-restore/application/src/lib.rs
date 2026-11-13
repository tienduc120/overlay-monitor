//! Fixture: the application rebuilds an aggregate without its checks.
use src_restore_domain::{Ticket, TicketId};

pub const fn reopen(raw: u64) -> Ticket {
    Ticket::restore(TicketId::new(raw))
}
