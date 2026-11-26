//! [`Crdt`] adapter for Automerge, using `AutoCommit`.
//!
//! `AutoCommit` keeps an implicit open transaction; [`AmTxn`] commits it on drop.
//! The patch log is inactive by default, so no patch bookkeeping is measured.
//! Text indexes are Unicode code points (Automerge's native default).

use std::hint::black_box;

use automerge::{
    ActorId, AutoCommit, Change, Cursor, ObjId, ObjType, ROOT, ReadDoc, ScalarValue,
    marks::{ExpandMark, Mark},
    sync::{self, SyncDoc},
    transaction::Transactable,
};

use crate::api::{Crdt, Feature, Txn, Value};

pub struct AmDoc {
    doc: AutoCommit,
}

impl AmDoc {
    pub fn inner(&self) -> &AutoCommit {
        &self.doc
    }

    pub fn inner_mut(&mut self) -> &mut AutoCommit {
        &mut self.doc
    }
}

/// Actor ID for a trace agent. Matches the Eg-walker paper's converter, which
/// used the big-endian bytes of a 64-bit integer.
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
        Ok(Some((automerge::Value::Object(ty), id))) if ty == expected => Some(id),
        _ => None,
    }
}

impl Crdt for AmDoc {
    const NAME: &'static str = "automerge";

    type Text = ObjId;
    type List = ObjId;
    type Map = ObjId;
    type Cursor = Cursor;
    type Change = Change;
    type SyncState = sync::State;
    type Txn<'a> = AmTxn<'a>;

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

    /// Like the battery's converter, a one-byte actor `0xff` creates the text
    /// object. Trace agents use 8-byte actors, so it can't collide with them.
    fn trace_root(text: &str) -> Self {
        let mut doc = AutoCommit::new();
        doc.set_actor(ActorId::from(&[0xff][..]));
        doc.put_object(ROOT, text, ObjType::Text).unwrap();
        doc.commit();
        Self { doc }
    }

    fn supports(feature: Feature) -> bool {
        match feature {
            Feature::Marks | Feature::Cursors | Feature::SyncProtocol => true,
        }
    }

    fn transact(&mut self) -> AmTxn<'_> {
        AmTxn { doc: &mut self.doc }
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
        self.doc.get_last_local_change()
    }

    fn changes(&mut self) -> Vec<Change> {
        self.doc.get_changes(&[])
    }

    fn apply_changes(&mut self, changes: Vec<Change>) {
        self.doc.apply_changes(changes).unwrap();
    }

    /// Automerge's sync protocol. Messages are encoded to bytes and decoded
    /// again, as they would be on a network, so the cost is comparable with
    /// libraries whose sync works on byte buffers.
    fn sync(&mut self, state: &mut sync::State, other: &mut Self, other_state: &mut sync::State) {
        loop {
            let mut progressed = false;
            if let Some(msg) = self.doc.sync().generate_sync_message(state) {
                let msg = sync::Message::decode(&msg.encode()).unwrap();
                other
                    .doc
                    .sync()
                    .receive_sync_message(other_state, msg)
                    .unwrap();
                progressed = true;
            }
            if let Some(msg) = other.doc.sync().generate_sync_message(other_state) {
                let msg = sync::Message::decode(&msg.encode()).unwrap();
                self.doc.sync().receive_sync_message(state, msg).unwrap();
                progressed = true;
            }
            if !progressed {
                break;
            }
        }
    }

    fn same_version(&mut self, other: &mut Self) -> bool {
        self.doc.get_heads() == other.doc.get_heads()
    }
}

pub struct AmTxn<'a> {
    doc: &'a mut AutoCommit,
}

impl Drop for AmTxn<'_> {
    fn drop(&mut self) {
        self.doc.commit();
    }
}

impl Txn<AmDoc> for AmTxn<'_> {
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
