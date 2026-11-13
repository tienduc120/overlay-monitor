use super::{rename, validate};

#[test]
fn the_witness_vocabulary_becomes_the_new_service_and_aggregate() {
    let src = "use example_domain::{Note, NoteId};\n// crate example-domain\nfn create_note(note: &Note) {}\n";

    let out = rename(src, "order-intake", "OrderLine");

    assert_eq!(
        out,
        "use order_intake_domain::{OrderLine, OrderLineId};\n// crate order-intake-domain\n\
         fn create_order_line(order_line: &OrderLine) {}\n"
    );
}

#[test]
fn a_name_that_contains_a_witness_word_is_not_renamed_twice() {
    assert_eq!(rename("example-domain", "notes", "Entry"), "notes-domain");
    assert_eq!(rename("Note note", "billing", "Footnote"), "Footnote footnote");
}

#[test]
fn paths_are_renamed_the_same_way() {
    assert_eq!(
        rename("application/src/create_note/tests.rs", "billing", "Invoice"),
        "application/src/create_invoice/tests.rs"
    );
}

#[test]
fn names_must_be_kebab_case_and_aggregates_pascal_case() {
    validate("billing", "Invoice").unwrap();
    validate("order-intake", "OrderLine").unwrap();
    assert!(validate("Billing", "Invoice").is_err());
    assert!(validate("billing_x", "Invoice").is_err());
    assert!(validate("billing", "invoice").is_err());
    assert!(
        validate("example", "Note").is_err(),
        "the witness name is taken by definition"
    );
}
