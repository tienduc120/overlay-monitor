//! Supervised concurrency (ADR-0009). Every task belongs to a [`Tasks`] set: the first failure or panic cancels the
//! rest, [`Tasks::wait`] returns only when all have stopped, and dropping the set aborts whatever is still running.
//! This is the only crate allowed to start a task; clippy.toml refuses `tokio::spawn` everywhere else.

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use tokio::task::{Id, JoinError, JoinSet};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// The cause is the error's `source()`, not repeated in its message.
    #[error("{name} failed")]
    Failed { name: &'static str, source: BoxError },
    #[error("{name} panicked: {message}")]
    Panicked { name: &'static str, message: String },
    #[error("{name} did not stop within {grace:?} of cancellation and was aborted")]
    Stuck { name: &'static str, grace: Duration },
}

#[derive(Debug, thiserror::Error)]
#[error("{} task(s) failed: {}", .0.len(), .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
pub struct TaskErrors(pub Vec<TaskError>);

pub struct Tasks {
    token: CancellationToken,
    grace: Duration,
    set: JoinSet<Result<(), BoxError>>,
    names: HashMap<Id, &'static str>,
}

impl Tasks {
    /// The set's tasks stop when `parent` is cancelled, or when one of them fails. `grace` is how long they may take
    /// to return once cancelled; there is no default, because a task that ignores its token would hang shutdown.
    pub fn new(parent: &CancellationToken, grace: Duration) -> Self {
        Self {
            token: parent.child_token(),
            grace,
            set: JoinSet::new(),
            names: HashMap::new(),
        }
    }

    /// Starts `task` with a token it must watch: when the token is cancelled the task returns, usually `Ok(())`.
    pub fn spawn<F, Fut>(&mut self, name: &'static str, task: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = Result<(), BoxError>> + Send + 'static,
    {
        let future = task(self.token.clone());
        #[expect(
            clippy::disallowed_methods,
            reason = "platform-task is the one place that starts tasks (ADR-0009)"
        )]
        let handle = self.set.spawn(future);
        self.names.insert(handle.id(), name);
    }

    /// Waits for every task. The first failure or panic cancels the others; once cancelled, tasks get the grace
    /// period to return and are then aborted and reported as stuck. Every failure is returned.
    pub async fn wait(mut self) -> Result<(), TaskErrors> {
        let mut errors = Vec::new();
        let mut deadline = None;
        loop {
            if deadline.is_none() && self.token.is_cancelled() {
                deadline = Some(Instant::now() + self.grace);
            }
            let joined = match deadline {
                None => tokio::select! {
                    joined = self.set.join_next_with_id() => joined,
                    // Cancelled from outside: loop round to start the grace period.
                    () = self.token.cancelled() => continue,
                },
                Some(at) => match tokio::time::timeout_at(at, self.set.join_next_with_id()).await {
                    Ok(joined) => joined,
                    Err(_elapsed) => {
                        errors.extend(self.abort_stuck().await);
                        break;
                    }
                },
            };
            let Some(joined) = joined else { break };
            if let Some(error) = self.failure(joined) {
                self.token.cancel();
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(TaskErrors(errors))
        }
    }

    fn failure(&self, joined: Result<(Id, Result<(), BoxError>), JoinError>) -> Option<TaskError> {
        match joined {
            Ok((id, Err(source))) => Some(TaskError::Failed {
                name: self.name(id),
                source,
            }),
            Err(e) if e.is_panic() => {
                let name = self.name(e.id());
                Some(TaskError::Panicked {
                    name,
                    message: panic_message(e.into_panic()),
                })
            }
            // Finished, or aborted by `abort_stuck`, which reports those itself.
            Ok((_, Ok(()))) | Err(_) => None,
        }
    }

    /// Aborts what is still running after the grace period and names each one.
    async fn abort_stuck(&mut self) -> Vec<TaskError> {
        self.set.abort_all();
        let mut errors = Vec::new();
        while let Some(joined) = self.set.join_next_with_id().await {
            match joined {
                Err(e) if e.is_cancelled() => errors.push(TaskError::Stuck {
                    name: self.name(e.id()),
                    grace: self.grace,
                }),
                other => errors.extend(self.failure(other)),
            }
        }
        errors
    }

    fn name(&self, id: Id) -> &'static str {
        self.names.get(&id).copied().unwrap_or("unnamed")
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map_or_else(|| "a non-string panic".to_owned(), |s| (*s).to_owned()),
    }
}

#[cfg(test)]
mod tests;
