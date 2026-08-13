#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod content;
mod domain;

pub(crate) use domain::{ContentError, Domain};
