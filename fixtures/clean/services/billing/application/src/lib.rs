//! Clean twin: a use case builds the aggregate through `new`.
use billing_domain::{Invoice, InvoiceId};

pub fn draft(id: &str, total_cents: u64) -> Option<u64> {
    Invoice::new(InvoiceId::new(id)?, total_cents).map(|invoice| invoice.total_cents())
}
