//! Clean twin: only infrastructure rebuilds a stored aggregate.
use billing_domain::{Invoice, InvoiceId};

pub fn load(id: &str, total_cents: u64) -> Option<u64> {
    InvoiceId::new(id).map(|id| Invoice::restore(id, total_cents).total_cents())
}
