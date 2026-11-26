//! [`crate::api::Crdt`] implementations, one per library.

pub mod automerge;
pub mod automerge_hashless;
pub mod yrs;

pub use self::automerge::AmDoc;
pub use self::automerge_hashless::AmHashlessDoc;
pub use self::yrs::YrsDoc;
