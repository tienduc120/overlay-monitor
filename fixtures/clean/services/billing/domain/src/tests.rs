use super::{Invoice, InvoiceId};

#[test]
fn an_empty_invoice_is_refused() {
    assert_eq!(Invoice::new(InvoiceId::new("i-1").unwrap(), 0), None);
}
