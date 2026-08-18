#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod domain;
mod interaction_status;

pub(crate) use domain::{Domain, InteractionStatusError};
