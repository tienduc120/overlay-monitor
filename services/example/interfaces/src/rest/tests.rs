use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use example_application::CreateNote;
use example_application::testing::{FakeNoteRepository, SaveOutcome};
use http_body_util::BodyExt as _;
use platform_clock::{FixedClock, Timestamp};
use tower::ServiceExt as _;

use super::router;

async fn post(outcome: SaveOutcome, body: &str) -> (StatusCode, String) {
    send(outcome, "application/json", body).await
}

async fn send(outcome: SaveOutcome, content_type: &str, body: &str) -> (StatusCode, String) {
    let clock = FixedClock(Timestamp::from_second(1_700_000_000).unwrap());
    let app = router(Arc::new(CreateNote::new(FakeNoteRepository::new(outcome), clock)));
    let request = Request::post("/notes")
        .header("content-type", content_type)
        .body(Body::from(body.to_owned()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn a_body_that_is_not_json_keeps_the_status_axum_gives_it() {
    let (status, _) = send(SaveOutcome::Saved, "text/plain", r#"{"id":"n-1","name":"groceries"}"#).await;

    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn a_created_note_is_201_with_its_id() {
    let created = post(SaveOutcome::Saved, r#"{"id":"n-1","name":"groceries"}"#).await;

    assert_eq!(created, (StatusCode::CREATED, r#"{"id":"n-1"}"#.to_owned()));
}

#[tokio::test]
async fn a_malformed_body_is_400() {
    assert_eq!(post(SaveOutcome::Saved, "{").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_invalid_note_is_400_and_says_why() {
    let (status, body) = post(SaveOutcome::Saved, r#"{"id":"n-1","name":" "}"#).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("name must not be empty"), "{body}");
}

#[tokio::test]
async fn a_taken_id_is_409() {
    assert_eq!(
        post(SaveOutcome::Taken, r#"{"id":"n-1","name":"groceries"}"#).await.0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn a_storage_failure_is_500_and_never_shows_its_cause_to_the_client() {
    let (status, body) = post(SaveOutcome::Broken, r#"{"id":"n-1","name":"groceries"}"#).await;

    assert_eq!(
        (status, body.as_str()),
        (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
    );
}
