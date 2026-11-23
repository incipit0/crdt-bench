//! Library-agnostic CRDT interface used by the benchmark battery.
//!
//! Every scenario is written once, generic over [`Crdt`], and instantiated per
//! library. Methods are monomorphised, so no dynamic dispatch happens inside
//! timed loops.
//!
//! Conventions:
//! - Text positions and lengths are Unicode code points. Adapters convert when
//!   their library counts in another unit.
//! - Writes happen through a [`Txn`], which commits when dropped. A benchmark
//!   that wants "one transaction per edit" opens one per edit.
//! - Optional capabilities are described by [`Feature`]. Scenarios that need a
//!   feature a library lacks are reported as unsupported for that library
//!   instead of being silently skipped.

/// A scalar value stored in a list or map.
#[derive(Clone, Copy, Debug)]
pub enum Value<'a> {
    Int(i64),
    Str(&'a str),
}

impl From<i64> for Value<'_> {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl<'a> From<&'a str> for Value<'a> {
    fn from(value: &'a str) -> Self {
        Self::Str(value)
    }
}

/// Capabilities that not every library has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    /// Rich-text formatting ranges on text.
    Marks,
    /// Stable positions in a sequence that survive concurrent edits.
    Cursors,
}

impl Feature {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Marks => "marks",
            Self::Cursors => "cursors",
        }
    }
}

pub trait Crdt: Sized + 'static {
    /// Short lowercase identifier, used as the last segment of benchmark names.
    const NAME: &'static str;

    type Text: Clone;
    type List: Clone;
    type Map: Clone;
    type Cursor;
    /// The unit `changes` returns and `apply_changes` accepts.
    type Change: Clone;
    /// Per-peer state kept between `sync` calls.
    type SyncState: Default;
    type Txn<'a>: Txn<Self>
    where
        Self: 'a;

    /// Empty document with a random agent.
    fn new() -> Self;
    /// Empty document whose local edits are attributed to `agent`.
    fn with_agent(agent: u64) -> Self;
    /// The document every branch of a replayed editing trace starts from: it
    /// holds an empty text object at the root under `text`. Its edits, if
    /// any, use an agent that no trace agent can collide with.
    fn trace_root(text: &str) -> Self;
    fn supports(feature: Feature) -> bool;

    /// Open a transaction. It commits when dropped.
    fn transact(&mut self) -> Self::Txn<'_>;

    // ----- object lookup -----
    /// The text object stored at the root under `name`, if there is one.
    fn get_text(&self, name: &str) -> Option<Self::Text>;
    /// The list object stored at the root under `name`, if there is one.
    fn get_list(&self, name: &str) -> Option<Self::List>;
    /// The document's root map.
    fn root_map(&self) -> Self::Map;

    // ----- reads -----
    fn text_len(&self, text: &Self::Text) -> usize;
    fn text(&self, text: &Self::Text) -> String;
    fn list_len(&self, list: &Self::List) -> usize;
    fn map_len(&self, map: &Self::Map) -> usize;
    /// Visit every value in `map` and return how many there were.
    fn map_visit_values(&self, map: &Self::Map) -> usize;
    /// Number of formatting marks covering the character at `index`.
    fn marks_at(&self, text: &Self::Text, index: usize) -> usize;
    fn list_cursor(&self, list: &Self::List, index: usize) -> Self::Cursor;
    fn list_cursor_position(&self, list: &Self::List, cursor: &Self::Cursor) -> usize;

    // ----- persistence -----
    fn save(&mut self) -> Vec<u8>;
    fn load(bytes: &[u8]) -> Self;
    /// Encode everything changed since the previous `save_incremental` (or
    /// since the document was created/loaded).
    fn save_incremental(&mut self) -> Vec<u8>;
    fn load_incremental(&mut self, bytes: &[u8]);

    // ----- replication -----
    /// Independent copy of this document with a fresh random agent.
    fn fork(&mut self) -> Self;
    /// Independent copy of this document whose new edits are attributed to `agent`.
    fn fork_as(&mut self, agent: u64) -> Self;
    /// Attribute this document's future local edits to `agent`. Libraries that
    /// fix the agent at creation replace the document with a copy, but only
    /// when the agent actually changes.
    fn set_agent(&mut self, agent: u64);
    /// The cheapest way the library offers to duplicate a document in memory.
    fn duplicate(&self) -> Self;
    /// Pull every change `other` has that `self` lacks.
    fn merge(&mut self, other: &mut Self);
    /// Ask the document to remember each local transaction so that
    /// `last_local_change` can return it. Some libraries only do this on request.
    fn record_local_changes(&mut self) {}
    fn last_local_change(&mut self) -> Option<Self::Change>;
    /// Every change in the document, in an order `apply_changes` accepts.
    fn changes(&mut self) -> Vec<Self::Change>;
    fn apply_changes(&mut self, changes: Vec<Self::Change>);
    /// Run the library's sync protocol between two peers until both are up to date.
    fn sync(
        &mut self,
        state: &mut Self::SyncState,
        other: &mut Self,
        other_state: &mut Self::SyncState,
    );
    /// True when both documents have seen the same set of changes.
    fn same_version(&mut self, other: &mut Self) -> bool;
}

/// Write operations, available inside a transaction.
pub trait Txn<D: Crdt> {
    /// Create a text object at the root under `name`.
    fn create_text(&mut self, name: &str) -> D::Text;
    /// Create a list object at the root under `name`.
    fn create_list(&mut self, name: &str) -> D::List;

    fn text_len(&self, text: &D::Text) -> usize;
    fn list_len(&self, list: &D::List) -> usize;

    /// Delete `delete` characters at `pos`, then insert `insert` there.
    fn splice_text(&mut self, text: &D::Text, pos: usize, delete: usize, insert: &str);
    /// Delete `delete` items at `pos`, then insert `values` there.
    fn splice_list(&mut self, list: &D::List, pos: usize, delete: usize, values: &[Value<'_>]);
    /// Replace the item at `pos`.
    fn list_set(&mut self, list: &D::List, pos: usize, value: Value<'_>);
    fn map_put(&mut self, map: &D::Map, key: &str, value: Value<'_>);
    /// Put a new empty map under `key` and return it.
    fn map_put_map(&mut self, map: &D::Map, key: &str) -> D::Map;
    /// Add a boolean formatting mark over `start..end` that expands on both sides.
    fn mark(&mut self, text: &D::Text, start: usize, end: usize, name: &str);
}
