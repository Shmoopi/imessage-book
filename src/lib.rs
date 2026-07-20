//! imessage-book: turn an iMessage conversation into a book.
//!
//! The binary (`main.rs`) is a thin CLI over these modules; exposing them as a library
//! lets integration tests and examples drive the renderers without a live database.

pub mod assemble;
pub mod attachments;
pub mod build;
pub mod cli;
pub mod config;
pub mod db;
pub mod model;
pub mod preview;
pub mod render;

#[cfg(test)]
mod fixture_tests;
