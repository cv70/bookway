#![allow(clippy::module_inception)] // Domain 定义在 domain/domain.rs
mod comment;
mod domain;

pub(crate) use domain::{CommentError, Domain};
