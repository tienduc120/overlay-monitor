use example_domain::{Note, NoteId};
use platform_clock::{FixedClock, Timestamp};

use super::{CreateNote, CreateNoteCommand, CreateNoteError};
use crate::SaveError;
use crate::testing::{FakeNoteRepository, SaveOutcome};

fn at() -> Timestamp {
    Timestamp::from_second(1_700_000_000).unwrap()
}

fn use_case(outcome: SaveOutcome) -> CreateNote<FakeNoteRepository, FixedClock> {
    CreateNote::new(FakeNoteRepository::new(outcome), FixedClock(at()))
}

fn command(id: &str, name: &str) -> CreateNoteCommand {
    CreateNoteCommand {
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

#[tokio::test]
async fn a_valid_command_saves_one_note_stamped_by_the_clock() {
    let use_case = use_case(SaveOutcome::Saved);

    let result = use_case.execute(command("n-1", "groceries")).await.unwrap();

    assert_eq!(result.id, "n-1");
    let expected = Note::new(NoteId::new("n-1").unwrap(), "groceries", at()).unwrap();
    assert_eq!(
        use_case.repository().saved(),
        vec![expected],
        "saved exactly once, as built"
    );
}

#[tokio::test]
async fn an_invalid_command_is_refused_before_anything_is_saved() {
    let use_case = use_case(SaveOutcome::Saved);

    let err = use_case.execute(command("n-1", "  ")).await.unwrap_err();

    assert!(matches!(err, CreateNoteError::Invalid(_)), "{err:?}");
    assert_eq!(use_case.repository().saved(), vec![], "nothing saved");
}

#[tokio::test]
async fn a_taken_id_is_a_conflict_not_a_storage_failure() {
    let err = use_case(SaveOutcome::Taken)
        .execute(command("n-1", "groceries"))
        .await
        .unwrap_err();

    assert!(
        matches!(&err, CreateNoteError::AlreadyExists(id) if id == "n-1"),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_storage_failure_keeps_its_cause() {
    let err = use_case(SaveOutcome::Broken)
        .execute(command("n-1", "groceries"))
        .await
        .unwrap_err();

    assert!(
        matches!(err, CreateNoteError::Storage(SaveError::Storage(_))),
        "{err:?}"
    );
}
