//! This service over HTTP. Only this layer maps an outcome to a status code (ADR-0011).

mod rest;

pub use rest::router;
