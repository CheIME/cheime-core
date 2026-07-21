#![allow(unsafe_code)]

pub mod format;
pub mod reader;

pub use format::write_tidex;
pub use reader::{TidexError, TidexReader};


