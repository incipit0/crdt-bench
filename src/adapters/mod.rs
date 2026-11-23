//! [`crate::api::Crdt`] implementations, one per library.

pub mod automerge;
pub mod yrs;

pub use self::automerge::AmDoc;
pub use self::yrs::YrsDoc;
