//! [`Crdt`] adapter for Automerge hashless: the document type on Automerge's
//! upstream `hashless` branch that keeps only the change hashes it needs
//! (crate imported as `automerge_hashless`; the type is the branch's
//! `next::AutoCommit`). It names history by `ChangeId`.
//!
//! Differences from [`super::AmDoc`]:
//! - `save` writes the bundle format, the branch's default for hashless
//!   documents.
//! - Changes are the classic per-change `Change`, via the `*_legacy` getters,
//!   so the change-shipping benchmarks move the same units as the release
//!   adapter. The native unit would be a `Bundle`.
//! - There is no sync protocol for hashless documents: `automerge-sync` only
//!   takes audited ones, and the branch's experimental `sync_fragments`
//!   sends "everything since my last call" without learning what the peer
//!   has, so it fails after a load or fork, with a second peer, or when a
//!   message is lost. Reconciliation looks meant for Subduction's sedimentree
//!   sync, which fingerprints the fragments hashless documents keep hashes for; that
//!   needs a port of Subduction's Automerge ingest to this API. Until then
//!   [`Feature::SyncProtocol`] is unsupported and the sync benchmarks are
//!   reported as such.

use std::hint::black_box;

use automerge_hashless::{
    ActorId, Change, Cursor, ObjId, ObjType, ROOT, ScalarValue,
    marks::{ExpandMark, Mark},
    next::{AutoCommit, ReadDoc, transaction::Transactable},
};

use crate::api::{Crdt, Feature, Txn, Value};

pub struct AmHashlessDoc {
    doc: AutoCommit,
}

impl AmHashlessDoc {
    pub fn inner(&self) -> &AutoCommit {
        &self.doc
    }

    pub fn inner_mut(&mut self) -> &mut AutoCommit {
        &mut self.doc
    }
}

/// Same actor bytes as [`super::AmDoc`], so both adapters replay traces identically.
fn actor(agent: u64) -> ActorId {
    ActorId::from(agent.to_be_bytes().as_slice())
}

fn scalar(value: Value<'_>) -> ScalarValue {
    match value {
        Value::Int(i) => ScalarValue::Int(i),
        Value::Str(s) => ScalarValue::from(s),
    }
}

fn root_object(doc: &AutoCommit, name: &str, expected: ObjType) -> Option<ObjId> {
    match doc.get(ROOT, name) {
        Ok(Some((automerge_hashless::Value::Object(ty), id))) if ty == expected => Some(id),
        _ => None,
    }
}

/// No sync protocol for hashless documents; see the module docs.
#[derive(Default)]
pub struct NoSyncState;

impl Crdt for AmHashlessDoc {
    const NAME: &'static str = "automerge-hashless";

    type Text = ObjId;
    type List = ObjId;
    type Map = ObjId;
    type Cursor = Cursor;
    type Change = Change;
    type SyncState = NoSyncState;
    type Txn<'a> = AmHashlessTxn<'a>;

    fn new() -> Self {
        Self {
            doc: AutoCommit::new(),
        }
    }

    fn with_agent(agent: u64) -> Self {
        let mut doc = AutoCommit::new();
        doc.set_actor(actor(agent));
        Self { doc }
    }

    fn trace_root(text: &str) -> Self {
        let mut doc = AutoCommit::new();
        doc.set_actor(ActorId::from(&[0xff][..]));
        doc.put_object(ROOT, text, ObjType::Text).unwrap();
        doc.commit();
        Self { doc }
    }

    fn supports(feature: Feature) -> bool {
        match feature {
            Feature::Marks | Feature::Cursors => true,
            Feature::SyncProtocol => false,
        }
    }

    fn transact(&mut self) -> AmHashlessTxn<'_> {
        AmHashlessTxn { doc: &mut self.doc }
    }

    fn get_text(&self, name: &str) -> Option<ObjId> {
        root_object(&self.doc, name, ObjType::Text)
    }

    fn get_list(&self, name: &str) -> Option<ObjId> {
        root_object(&self.doc, name, ObjType::List)
    }

    fn root_map(&self) -> ObjId {
        ROOT
    }

    fn text_len(&self, text: &ObjId) -> usize {
        self.doc.length(text)
    }

    fn text(&self, text: &ObjId) -> String {
        self.doc.text(text).unwrap()
    }

    fn list_len(&self, list: &ObjId) -> usize {
        self.doc.length(list)
    }

    fn map_len(&self, map: &ObjId) -> usize {
        self.doc.length(map)
    }

    fn map_visit_values(&self, map: &ObjId) -> usize {
        self.doc.values(map).map(black_box).count()
    }

    fn marks_at(&self, text: &ObjId, index: usize) -> usize {
        self.doc.get_marks(text, index, None).unwrap().iter().count()
    }

    fn list_cursor(&self, list: &ObjId, index: usize) -> Cursor {
        self.doc.get_cursor(list, index, None).unwrap()
    }

    fn list_cursor_position(&self, list: &ObjId, cursor: &Cursor) -> usize {
        self.doc.get_cursor_position(list, cursor, None).unwrap()
    }

    fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    fn load(bytes: &[u8]) -> Self {
        Self {
            doc: AutoCommit::load(bytes).unwrap(),
        }
    }

    fn save_incremental(&mut self) -> Vec<u8> {
        self.doc.save_incremental()
    }

    fn load_incremental(&mut self, bytes: &[u8]) {
        self.doc.load_incremental(bytes).unwrap();
    }

    fn fork(&mut self) -> Self {
        Self {
            doc: self.doc.fork(),
        }
    }

    fn fork_as(&mut self, agent: u64) -> Self {
        let mut doc = self.doc.fork();
        doc.set_actor(actor(agent));
        Self { doc }
    }

    fn set_agent(&mut self, agent: u64) {
        self.doc.set_actor(actor(agent));
    }

    fn duplicate(&self) -> Self {
        Self {
            doc: self.doc.clone(),
        }
    }

    fn merge(&mut self, other: &mut Self) {
        self.doc.merge(&mut other.doc).unwrap();
    }

    fn last_local_change(&mut self) -> Option<Change> {
        self.doc.get_last_local_change_legacy().unwrap()
    }

    fn changes(&mut self) -> Vec<Change> {
        self.doc.get_changes(&[]).unwrap()
    }

    fn apply_changes(&mut self, changes: Vec<Change>) {
        self.doc.apply_changes(changes).unwrap();
    }

    fn sync(&mut self, _: &mut NoSyncState, _: &mut Self, _: &mut NoSyncState) {
        unreachable!("automerge-hashless has no sync protocol; sync benchmarks require Feature::SyncProtocol")
    }

    fn same_version(&mut self, other: &mut Self) -> bool {
        self.doc.get_heads() == other.doc.get_heads()
    }
}

pub struct AmHashlessTxn<'a> {
    doc: &'a mut AutoCommit,
}

impl Drop for AmHashlessTxn<'_> {
    fn drop(&mut self) {
        self.doc.commit();
    }
}

impl Txn<AmHashlessDoc> for AmHashlessTxn<'_> {
    fn create_text(&mut self, name: &str) -> ObjId {
        self.doc.put_object(ROOT, name, ObjType::Text).unwrap()
    }

    fn create_list(&mut self, name: &str) -> ObjId {
        self.doc.put_object(ROOT, name, ObjType::List).unwrap()
    }

    fn text_len(&self, text: &ObjId) -> usize {
        self.doc.length(text)
    }

    fn list_len(&self, list: &ObjId) -> usize {
        self.doc.length(list)
    }

    fn splice_text(&mut self, text: &ObjId, pos: usize, delete: usize, insert: &str) {
        self.doc
            .splice_text(text, pos, delete as isize, insert)
            .unwrap();
    }

    fn splice_list(&mut self, list: &ObjId, pos: usize, delete: usize, values: &[Value<'_>]) {
        self.doc
            .splice(list, pos, delete as isize, values.iter().map(|v| scalar(*v)))
            .unwrap();
    }

    fn list_set(&mut self, list: &ObjId, pos: usize, value: Value<'_>) {
        self.doc.put(list, pos, scalar(value)).unwrap();
    }

    fn map_put(&mut self, map: &ObjId, key: &str, value: Value<'_>) {
        self.doc.put(map, key, scalar(value)).unwrap();
    }

    fn map_put_map(&mut self, map: &ObjId, key: &str) -> ObjId {
        self.doc.put_object(map, key, ObjType::Map).unwrap()
    }

    fn mark(&mut self, text: &ObjId, start: usize, end: usize, name: &str) {
        self.doc
            .mark(
                text,
                Mark::new(name.to_string(), true, start, end),
                ExpandMark::Both,
            )
            .unwrap();
    }
}
