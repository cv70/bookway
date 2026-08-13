#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod domain;
mod media;

pub(crate) use domain::{Domain, MediaError};
