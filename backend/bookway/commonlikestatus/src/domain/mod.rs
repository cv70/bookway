#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod domain;
mod like_status;

pub(crate) use domain::{Domain, LikeStatusError};
