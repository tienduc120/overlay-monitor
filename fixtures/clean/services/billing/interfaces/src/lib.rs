//! Clean twin: delivery maps an outcome to a status and sees only the application.
pub fn create_status(id: &str, total_cents: u64) -> u16 {
    if billing_application::draft(id, total_cents).is_some() { 201 } else { 400 }
}
