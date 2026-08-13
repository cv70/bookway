#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod bbs;
mod domain;

pub(crate) use domain::{BbsError, Domain};
