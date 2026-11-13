//! A sensor for a reading this machine cannot give, so the overlay shows `—` instead of failing to start.

use monitor_application::{ReadError, Sensor};

/// Always answers [`ReadError::Unavailable`] with the reason it was built with.
#[derive(Debug)]
pub struct UnavailableSensor(pub String);

impl Sensor for UnavailableSensor {
    fn read(&mut self) -> Result<f64, ReadError> {
        Err(ReadError::Unavailable(self.0.clone()))
    }
}

/// Keeps a sensor that opened, and turns an `open` failure into an [`UnavailableSensor`] naming it.
#[must_use]
pub fn or_unavailable<S: Sensor + 'static>(opened: Result<S, ReadError>) -> Box<dyn Sensor> {
    match opened {
        Ok(sensor) => Box::new(sensor),
        Err(error) => Box::new(UnavailableSensor(reason(error))),
    }
}

/// `Unavailable` already carries its reason; a failure's reason is its whole cause chain, which `Display` alone drops.
fn reason(error: ReadError) -> String {
    match error {
        ReadError::Unavailable(reason) => reason,
        ReadError::Failed(cause) => {
            let mut text = cause.to_string();
            let mut next = cause.source();
            while let Some(source) = next {
                text.push_str(": ");
                text.push_str(&source.to_string());
                next = source.source();
            }
            text
        }
    }
}

#[cfg(test)]
mod tests;
