#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod domain;
mod recall;

pub use domain::Domain;
