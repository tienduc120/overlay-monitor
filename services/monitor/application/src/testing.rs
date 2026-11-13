//! Hand-written fakes of this crate's ports, for other layers' tests (feature `test-support`).

use std::collections::VecDeque;

use crate::{ReadError, Sensor};

/// Answers in order, then keeps repeating the last answer.
#[derive(Debug)]
pub struct FakeSensor {
    queued: VecDeque<Result<f64, ReadError>>,
    last: Option<Result<f64, ReadError>>,
}

impl FakeSensor {
    pub fn new(results: Vec<Result<f64, ReadError>>) -> Self {
        Self {
            queued: results.into(),
            last: None,
        }
    }

    pub fn fixed(value: f64) -> Self {
        Self::new(vec![Ok(value)])
    }
}

/// `ReadError` is not `Clone`; a repeated `Failed` is rebuilt from its source's message.
fn repeat(answer: &Result<f64, ReadError>) -> Result<f64, ReadError> {
    match answer {
        Ok(v) => Ok(*v),
        Err(ReadError::Unavailable(message)) => Err(ReadError::Unavailable(message.clone())),
        Err(ReadError::Failed(source)) => Err(ReadError::Failed(source.to_string().into())),
    }
}

impl Sensor for FakeSensor {
    fn read(&mut self) -> Result<f64, ReadError> {
        if let Some(next) = self.queued.pop_front() {
            self.last = Some(repeat(&next));
            return next;
        }
        self.last.as_ref().map_or_else(
            || Err(ReadError::Unavailable(String::from("no answer programmed"))),
            repeat,
        )
    }
}
