//! Google Sheets functions that GridCraft doesn't have, kept out of the patch queue: each takes
//! plain values and returns values, so it is tested here on its own. GridCraft's function table
//! registers them through a small patch (patches/0023).

pub mod query;

pub use query::{QueryError, query};
