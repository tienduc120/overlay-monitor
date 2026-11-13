use example_application::{NoteRepository as _, SaveError};
use example_domain::{Note, NoteId};
use jiff::Timestamp;

use super::MemoryNoteRepository;

fn note(id: &str) -> Note {
    Note::new(
        NoteId::new(id).unwrap(),
        "groceries",
        Timestamp::from_second(1_700_000_000).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn a_new_note_is_saved_and_a_second_save_of_its_id_is_a_conflict() {
    let repo = MemoryNoteRepository::default();

    repo.save(&note("n-1")).await.unwrap();
    let again = repo.save(&note("n-1")).await;

    assert!(matches!(again, Err(SaveError::AlreadyExists)), "{again:?}");
}

#[tokio::test]
async fn different_ids_do_not_conflict() {
    let repo = MemoryNoteRepository::default();

    repo.save(&note("n-1")).await.unwrap();
    repo.save(&note("n-2")).await.unwrap();
}
