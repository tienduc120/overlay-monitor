use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{TaskError, Tasks};

const GRACE: Duration = Duration::from_secs(5);

fn root() -> CancellationToken {
    #[expect(clippy::disallowed_methods, reason = "a test is its own composition root")]
    CancellationToken::new()
}

#[tokio::test]
async fn tasks_that_all_finish_report_nothing() {
    let mut tasks = Tasks::new(&root(), GRACE);
    tasks.spawn("a", |_| async { Ok(()) });
    tasks.spawn("b", |_| async { Ok(()) });

    tasks.wait().await.unwrap();
}

#[tokio::test]
async fn the_first_failure_cancels_the_others_and_is_returned_by_name() {
    let stopped = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&stopped);
    let mut tasks = Tasks::new(&root(), GRACE);
    tasks.spawn("server", move |token| async move {
        token.cancelled().await;
        seen.store(true, Ordering::SeqCst);
        Ok(())
    });
    tasks.spawn("listener", |_| async { Err("address in use".into()) });

    let errors = tasks.wait().await.unwrap_err();

    assert!(stopped.load(Ordering::SeqCst), "the server was told to stop");
    assert_eq!(errors.0.len(), 1, "{errors:?}");
    assert!(
        matches!(&errors.0[0], TaskError::Failed { name: "listener", .. }),
        "{errors:?}"
    );
    let failed = &errors.0[0];
    assert_eq!(
        failed.to_string(),
        "listener failed",
        "the cause is the source, not repeated in the message"
    );
    assert_eq!(std::error::Error::source(failed).unwrap().to_string(), "address in use");
}

#[tokio::test]
async fn a_panic_becomes_an_error_that_names_the_task_and_cancels_the_rest() {
    let mut tasks = Tasks::new(&root(), GRACE);
    tasks.spawn("waiter", |token| async move {
        token.cancelled().await;
        Ok(())
    });
    tasks.spawn("buggy", |_| async { panic!("index out of range") });

    let errors = tasks.wait().await.unwrap_err();

    assert_eq!(errors.0.len(), 1, "{errors:?}");
    let TaskError::Panicked { name, message } = &errors.0[0] else {
        panic!("{errors:?}")
    };
    assert_eq!((*name, message.as_str()), ("buggy", "index out of range"));
}

#[tokio::test]
async fn cancelling_the_parent_stops_every_task_cleanly() {
    let parent = root();
    let mut tasks = Tasks::new(&parent, GRACE);
    tasks.spawn("a", |token| async move {
        token.cancelled().await;
        Ok(())
    });

    parent.cancel();

    tasks.wait().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_task_that_ignores_cancellation_is_aborted_after_the_grace_period_and_named() {
    let parent = root();
    let mut tasks = Tasks::new(&parent, GRACE);
    tasks.spawn("stubborn", |_| async {
        std::future::pending::<()>().await;
        Ok(())
    });

    parent.cancel();
    let waited = tokio::time::timeout(GRACE * 10, tasks.wait()).await;

    let errors = waited.expect("wait ends once the grace period is over").unwrap_err();
    assert!(
        matches!(errors.0.as_slice(), [TaskError::Stuck { name: "stubborn", .. }]),
        "{errors:?}"
    );
}
