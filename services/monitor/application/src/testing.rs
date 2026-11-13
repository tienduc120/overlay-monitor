//! Hand-written fakes of this crate's ports, for other layers' tests (feature `test-support`).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    DriverStatus, Extras, GpuPair, GpuSensors, LoadSettingsError, PawnIoStatus, ReadError, SaveSettingsError, Sensor,
    Settings, SettingsStore, StartupTask, StartupTaskError,
};

/// Answers in order, then keeps repeating the last answer.
#[derive(Debug)]
pub struct FakeSensor {
    queued: VecDeque<Result<f64, ReadError>>,
    last: Option<Result<f64, ReadError>>,
    extras: Extras,
}

impl FakeSensor {
    pub fn new(results: Vec<Result<f64, ReadError>>) -> Self {
        Self {
            queued: results.into(),
            last: None,
            extras: Extras::default(),
        }
    }

    pub fn fixed(value: f64) -> Self {
        Self::new(vec![Ok(value)])
    }

    /// Answers these extras after every `Ok`.
    #[must_use]
    pub fn with_extras(mut self, extras: Extras) -> Self {
        self.extras = extras;
        self
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

    fn extras(&self) -> Extras {
        self.extras.clone()
    }
}

#[derive(Debug, Default)]
struct Record {
    held: Option<Settings>,
    unreadable: bool,
    failing_saves: bool,
    saved: Vec<Settings>,
}

/// A settings store in memory. A clone shares the one record, so a test keeps a handle after moving a clone away
/// (UI thread only).
#[derive(Clone, Debug, Default)]
pub struct MemoryStore(Rc<RefCell<Record>>);

impl MemoryStore {
    fn with(record: Record) -> Self {
        Self(Rc::new(RefCell::new(record)))
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn holding(settings: Settings) -> Self {
        Self::with(Record {
            held: Some(settings),
            ..Record::default()
        })
    }

    pub fn unreadable() -> Self {
        Self::with(Record {
            unreadable: true,
            ..Record::default()
        })
    }

    pub fn failing_saves() -> Self {
        Self::with(Record {
            failing_saves: true,
            ..Record::default()
        })
    }

    /// Every save, in order.
    pub fn saved(&self) -> Vec<Settings> {
        self.0.borrow().saved.clone()
    }
}

impl SettingsStore for MemoryStore {
    fn load(&self) -> Result<Option<Settings>, LoadSettingsError> {
        let record = self.0.borrow();
        if record.unreadable {
            return Err(LoadSettingsError::Unreadable {
                kept_as: String::from("settings.toml.bad"),
                source: "not a settings file".into(),
            });
        }
        Ok(record.held.clone())
    }

    fn save(&self, settings: &Settings) -> Result<(), SaveSettingsError> {
        let mut record = self.0.borrow_mut();
        if record.failing_saves {
            return Err(SaveSettingsError("the disk is full".into()));
        }
        record.saved.push(settings.clone());
        record.held = Some(settings.clone());
        Ok(())
    }
}

/// Opens a fixed pair for any adapter and remembers which names were asked for. A clone shares the record.
#[derive(Clone, Debug, Default)]
pub struct FakeGpuSensors(Arc<Mutex<Vec<Option<String>>>>);

impl FakeGpuSensors {
    /// Every name `open` was called with, in order.
    pub fn opened(&self) -> Vec<Option<String>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl GpuSensors for FakeGpuSensors {
    fn open(&mut self, name: Option<&str>) -> GpuPair {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(name.map(str::to_owned));
        GpuPair {
            load: Box::new(FakeSensor::fixed(11.0)),
            temp: Box::new(FakeSensor::fixed(66.0)),
        }
    }
}

#[derive(Debug, Default)]
struct StartupRecord {
    failing: bool,
    calls: Vec<bool>,
}

/// A startup task that only records what it was told. A clone shares the one record.
#[derive(Clone, Debug, Default)]
pub struct FakeStartupTask(Arc<Mutex<StartupRecord>>);

impl FakeStartupTask {
    /// Refuses every `set`, still recording the call.
    pub fn failing() -> Self {
        Self(Arc::new(Mutex::new(StartupRecord {
            failing: true,
            calls: Vec::new(),
        })))
    }

    /// Every value `set` was called with, in order.
    pub fn calls(&self) -> Vec<bool> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).calls.clone()
    }
}

impl StartupTask for FakeStartupTask {
    fn set(&self, on: bool) -> Result<(), StartupTaskError> {
        let mut record = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        record.calls.push(on);
        if record.failing {
            return Err(StartupTaskError("access denied".into()));
        }
        Ok(())
    }
}

/// A driver that always answers the one status it was given.
#[derive(Clone, Debug)]
pub struct FakeDriverStatus(pub PawnIoStatus);

impl DriverStatus for FakeDriverStatus {
    fn pawnio(&self) -> PawnIoStatus {
        self.0.clone()
    }
}
