#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod domain;
mod growth;

pub(crate) use domain::{Domain, GrowthError};
