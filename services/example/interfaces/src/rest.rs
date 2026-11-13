use std::sync::Arc;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use example_application::{CreateNote, CreateNoteCommand, CreateNoteError, NoteRepository};
use platform_clock::Clock;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CreateRequest {
    id: String,
    name: String,
}

#[derive(Serialize)]
struct CreateResponse {
    id: String,
}

pub fn router<R, C>(create: Arc<CreateNote<R, C>>) -> Router
where
    R: NoteRepository + 'static,
    C: Clock + 'static,
{
    Router::new()
        .route("/notes", post(create_note::<R, C>))
        .with_state(create)
}

async fn create_note<R, C>(
    State(create): State<Arc<CreateNote<R, C>>>,
    body: Result<Json<CreateRequest>, JsonRejection>,
) -> Response
where
    R: NoteRepository,
    C: Clock,
{
    // axum knows why it refused the body: 400 bad JSON, 413 too large, 415 not JSON, 422 wrong shape.
    let Json(req) = match body {
        Ok(json) => json,
        Err(rejection) => return rejection.into_response(),
    };
    // Exhaustive on purpose: a new outcome does not compile until someone decides its status.
    match create
        .execute(CreateNoteCommand {
            id: req.id,
            name: req.name,
        })
        .await
    {
        Ok(created) => (StatusCode::CREATED, Json(CreateResponse { id: created.id })).into_response(),
        Err(e @ CreateNoteError::Invalid(_)) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        Err(CreateNoteError::AlreadyExists(_)) => (StatusCode::CONFLICT, "already exists").into_response(),
        Err(e @ CreateNoteError::Storage(_)) => {
            tracing::error!(error = ?e, "create note");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

#[cfg(test)]
mod tests;
