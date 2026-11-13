use alloc::string::String;

use jiff::Timestamp;

use super::{Note, NoteError, NoteId};

fn at() -> Timestamp {
    Timestamp::from_second(1_700_000_000).unwrap()
}

#[test]
fn a_note_keeps_its_trimmed_name_and_the_time_it_was_given() {
    let id = NoteId::new("n-1").unwrap();

    let note = Note::new(id.clone(), "  groceries ", at()).unwrap();

    assert_eq!((note.id(), note.name(), note.created_at()), (&id, "groceries", at()));
}

#[test]
fn a_blank_name_is_refused() {
    let id = NoteId::new("n-1").unwrap();

    assert_eq!(Note::new(id, "   ", at()), Err(NoteError::EmptyName));
}

#[test]
fn a_blank_id_is_refused() {
    assert_eq!(NoteId::new(" "), Err(NoteError::EmptyId));
}

#[test]
fn restore_rebuilds_a_stored_value_exactly_as_given() {
    let id = NoteId::new("n-1").unwrap();

    let note = Note::restore(id.clone(), String::from("groceries"), at());

    assert_eq!(note, Note::new(id, "groceries", at()).unwrap());
}
