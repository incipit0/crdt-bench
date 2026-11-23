//! [`Crdt`] adapter for Yrs (the Rust port of Yjs).
//!
//! Differences from Automerge worth knowing when reading results:
//! - Yrs counts text in UTF-8 bytes or UTF-16 units, never code points. Docs
//!   use UTF-16 and this adapter converts positions. For text without
//!   characters outside the BMP (all generated docs, most traces) the
//!   conversion is skipped entirely.
//! - Cloning a Yrs `Doc` shares state, so `fork` and `duplicate` encode the
//!   full state and apply it to a fresh `Doc`.
//! - Yrs doesn't keep per-transaction history. `changes` returns one update
//!   holding the whole document; `last_local_change` needs
//!   `record_local_changes` first.
//! - There's no in-place list "set": `list_set` is a delete followed by an insert.
//! - `same_version` compares state vectors, which ignore deletions.
//! - Persistence uses the v1 update encoding.

use std::{collections::HashMap, hint::black_box, sync::Arc};

use yrs::{
    Any, Array, ArrayRef, Assoc, ClientID, Doc, GetString, IndexedSequence, Map, MapPrelim, MapRef,
    OffsetKind, Options, Out, ReadTxn, StateVector, StickyIndex, Text, TextRef, Transact,
    TransactionMut, Update, WriteTxn,
    types::{Attrs, text::YChange},
    updates::{decoder::Decode, encoder::Encode},
};

use crate::api::{Crdt, Feature, Txn, Value};

const ROOT_MAP: &str = "root";

pub struct YrsDoc {
    doc: Doc,
    /// State vector at the last `save_incremental`.
    saved: StateVector,
    record_local: bool,
    last_update: Option<Vec<u8>>,
    astral: Astral,
}

/// A root text object. The name keys the code-point/UTF-16 index.
#[derive(Clone)]
pub struct YText {
    name: Arc<str>,
    text: TextRef,
}

fn options(client_id: Option<u64>) -> Options {
    let mut options = match client_id {
        Some(id) => Options::with_client_id(ClientID::new(id)),
        None => Options::default(),
    };
    options.offset_kind = OffsetKind::Utf16;
    options
}

/// Trace agents count from 0; Yrs client IDs are kept non-zero. Yrs client
/// IDs must fit in 53 bits.
fn client_id(agent: u64) -> u64 {
    agent + 1
}

fn any(value: Value<'_>) -> Any {
    match value {
        Value::Int(i) => Any::from(i),
        Value::Str(s) => Any::from(s),
    }
}

fn decode(bytes: &[u8]) -> Update {
    Update::decode_v1(bytes).expect("valid yrs v1 update")
}

impl YrsDoc {
    fn from_doc(doc: Doc, astral: Astral) -> Self {
        let saved = doc.transact().state_vector();
        Self {
            doc,
            saved,
            record_local: false,
            last_update: None,
            astral,
        }
    }

    fn copy(&self, client_id: Option<u64>) -> Self {
        let state = self
            .doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let doc = Doc::with_options(options(client_id));
        doc.transact_mut().apply_update(decode(&state)).unwrap();
        Self::from_doc(doc, self.astral.clone())
    }

    /// Apply an update from somewhere else. `source_clean` says whether the
    /// sender is known to have no astral characters.
    fn absorb(&mut self, bytes: &[u8], source_clean: bool) {
        self.doc.transact_mut().apply_update(decode(bytes)).unwrap();
        self.astral.after_remote(source_clean);
    }

    pub fn inner(&self) -> &Doc {
        &self.doc
    }
}

impl Crdt for YrsDoc {
    const NAME: &'static str = "yrs";

    type Text = YText;
    type List = ArrayRef;
    type Map = MapRef;
    type Cursor = StickyIndex;
    type Change = Vec<u8>;
    type SyncState = ();
    type Txn<'a> = YrsTxn<'a>;

    fn new() -> Self {
        Self::from_doc(Doc::with_options(options(None)), Astral::default())
    }

    fn with_agent(agent: u64) -> Self {
        Self::from_doc(
            Doc::with_options(options(Some(client_id(agent)))),
            Astral::default(),
        )
    }

    /// Root types in Yrs are created by name on first use and need no
    /// transaction, so the root document is empty: there is no root txn.
    fn trace_root(text: &str) -> Self {
        let doc = Self::from_doc(Doc::with_options(options(None)), Astral::default());
        doc.doc.get_or_insert_text(text);
        doc
    }

    fn supports(feature: Feature) -> bool {
        match feature {
            Feature::Marks | Feature::Cursors => true,
        }
    }

    fn transact(&mut self) -> YrsTxn<'_> {
        YrsTxn {
            txn: self.doc.transact_mut(),
            last_update: self.record_local.then_some(&mut self.last_update),
            astral: &mut self.astral,
        }
    }

    fn get_text(&self, name: &str) -> Option<YText> {
        Some(YText {
            name: name.into(),
            text: self.doc.get_or_insert_text(name),
        })
    }

    fn get_list(&self, name: &str) -> Option<ArrayRef> {
        Some(self.doc.get_or_insert_array(name))
    }

    fn root_map(&self) -> MapRef {
        self.doc.get_or_insert_map(ROOT_MAP)
    }

    fn text_len(&self, text: &YText) -> usize {
        let txn = self.doc.transact();
        self.astral.code_points(text, &txn)
    }

    fn text(&self, text: &YText) -> String {
        text.text.get_string(&self.doc.transact())
    }

    fn list_len(&self, list: &ArrayRef) -> usize {
        list.len(&self.doc.transact()) as usize
    }

    fn map_len(&self, map: &MapRef) -> usize {
        map.len(&self.doc.transact()) as usize
    }

    fn map_visit_values(&self, map: &MapRef) -> usize {
        let txn = self.doc.transact();
        map.values(&txn).map(black_box).count()
    }

    /// Yrs has no point query for formatting, so this walks the text's delta.
    fn marks_at(&self, text: &YText, index: usize) -> usize {
        let txn = self.doc.transact();
        let target = self.astral.utf16_read(text, &txn, index) as usize;
        let mut pos = 0;
        for chunk in text.text.diff(&txn, YChange::identity) {
            let len = match &chunk.insert {
                Out::Any(Any::String(s)) => s.chars().map(char::len_utf16).sum(),
                _ => 1,
            };
            if target < pos + len {
                return chunk.attributes.map_or(0, |attrs| attrs.len());
            }
            pos += len;
        }
        0
    }

    fn list_cursor(&self, list: &ArrayRef, index: usize) -> StickyIndex {
        list.sticky_index(&self.doc.transact(), index as u32, Assoc::After)
            .unwrap()
    }

    fn list_cursor_position(&self, _list: &ArrayRef, cursor: &StickyIndex) -> usize {
        cursor.get_offset(&self.doc.transact()).unwrap().index as usize
    }

    fn save(&mut self) -> Vec<u8> {
        self.doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default())
    }

    fn load(bytes: &[u8]) -> Self {
        let doc = Doc::with_options(options(None));
        doc.transact_mut().apply_update(decode(bytes)).unwrap();
        Self::from_doc(doc, Astral::unknown())
    }

    fn save_incremental(&mut self) -> Vec<u8> {
        let txn = self.doc.transact();
        let update = txn.encode_diff_v1(&self.saved);
        self.saved = txn.state_vector();
        update
    }

    fn load_incremental(&mut self, bytes: &[u8]) {
        self.absorb(bytes, false);
    }

    fn fork(&mut self) -> Self {
        self.copy(None)
    }

    fn fork_as(&mut self, agent: u64) -> Self {
        self.copy(Some(client_id(agent)))
    }

    /// Yrs fixes the client ID when a `Doc` is created, so a different agent
    /// means copying the document.
    fn set_agent(&mut self, agent: u64) {
        let id = client_id(agent);
        if self.doc.client_id() != ClientID::new(id) {
            *self = self.copy(Some(id));
        }
    }

    fn duplicate(&self) -> Self {
        self.copy(None)
    }

    fn merge(&mut self, other: &mut Self) {
        let sv = self.doc.transact().state_vector();
        let update = other.doc.transact().encode_diff_v1(&sv);
        self.absorb(&update, other.astral.is_clean());
    }

    fn record_local_changes(&mut self) {
        self.record_local = true;
    }

    fn last_local_change(&mut self) -> Option<Vec<u8>> {
        self.last_update.clone()
    }

    fn changes(&mut self) -> Vec<Vec<u8>> {
        vec![self.save()]
    }

    fn apply_changes(&mut self, changes: Vec<Vec<u8>>) {
        for change in changes {
            self.absorb(&change, false);
        }
    }

    /// One round of the y-sync protocol: each side sends its state vector and
    /// receives the missing updates. Everything goes through byte buffers.
    fn sync(&mut self, _: &mut (), other: &mut Self, _: &mut ()) {
        let sv_self = self.doc.transact().state_vector().encode_v1();
        let sv_other = other.doc.transact().state_vector().encode_v1();
        let for_other = self
            .doc
            .transact()
            .encode_diff_v1(&StateVector::decode_v1(&sv_other).unwrap());
        let for_self = other
            .doc
            .transact()
            .encode_diff_v1(&StateVector::decode_v1(&sv_self).unwrap());
        let (self_clean, other_clean) = (self.astral.is_clean(), other.astral.is_clean());
        other.absorb(&for_other, self_clean);
        self.absorb(&for_self, other_clean);
    }

    fn same_version(&mut self, other: &mut Self) -> bool {
        self.doc.transact().state_vector() == other.doc.transact().state_vector()
    }
}

pub struct YrsTxn<'a> {
    txn: TransactionMut<'a>,
    last_update: Option<&'a mut Option<Vec<u8>>>,
    astral: &'a mut Astral,
}

impl Drop for YrsTxn<'_> {
    fn drop(&mut self) {
        if let Some(slot) = self.last_update.take() {
            *slot = Some(self.txn.encode_update_v1());
        }
    }
}

impl Txn<YrsDoc> for YrsTxn<'_> {
    fn create_text(&mut self, name: &str) -> YText {
        YText {
            name: name.into(),
            text: self.txn.get_or_insert_text(name),
        }
    }

    fn create_list(&mut self, name: &str) -> ArrayRef {
        self.txn.get_or_insert_array(name)
    }

    fn text_len(&self, text: &YText) -> usize {
        self.astral.code_points(text, &self.txn)
    }

    fn list_len(&self, list: &ArrayRef) -> usize {
        list.len(&self.txn) as usize
    }

    fn splice_text(&mut self, text: &YText, pos: usize, delete: usize, insert: &str) {
        let (pos16, delete16) = self.astral.splice(text, &self.txn, pos, delete, insert);
        if delete16 > 0 {
            text.text.remove_range(&mut self.txn, pos16, delete16);
        }
        if !insert.is_empty() {
            text.text.insert(&mut self.txn, pos16, insert);
        }
    }

    fn splice_list(&mut self, list: &ArrayRef, pos: usize, delete: usize, values: &[Value<'_>]) {
        if delete > 0 {
            list.remove_range(&mut self.txn, pos as u32, delete as u32);
        }
        if !values.is_empty() {
            list.insert_range(&mut self.txn, pos as u32, values.iter().map(|v| any(*v)));
        }
    }

    fn list_set(&mut self, list: &ArrayRef, pos: usize, value: Value<'_>) {
        list.remove(&mut self.txn, pos as u32);
        list.insert(&mut self.txn, pos as u32, any(value));
    }

    fn map_put(&mut self, map: &MapRef, key: &str, value: Value<'_>) {
        map.insert(&mut self.txn, key, any(value));
    }

    fn map_put_map(&mut self, map: &MapRef, key: &str) -> MapRef {
        map.insert(&mut self.txn, key, MapPrelim::default())
    }

    fn mark(&mut self, text: &YText, start: usize, end: usize, name: &str) {
        let start16 = self.astral.utf16_read(text, &self.txn, start);
        let end16 = self.astral.utf16_read(text, &self.txn, end);
        let attrs = Attrs::from([(Arc::<str>::from(name), Any::Bool(true))]);
        text.text
            .format(&mut self.txn, start16, end16 - start16, attrs);
    }
}

/// Positions of characters outside the BMP, per text, in code points.
/// Each one takes two UTF-16 units, so these are all that's needed to convert
/// between code-point and UTF-16 indexes.
#[derive(Clone, Default)]
struct Astral {
    /// When true, texts missing from `by_text` must be scanned before use.
    /// When false, a missing text has no astral characters.
    unknown: bool,
    by_text: HashMap<Arc<str>, Vec<usize>>,
}

fn has_astral(s: &str) -> bool {
    // Astral characters are exactly the 4-byte UTF-8 sequences.
    s.bytes().any(|b| b >= 0xF0)
}

fn scan(s: &str) -> Vec<usize> {
    s.chars()
        .enumerate()
        .filter(|(_, c)| c.len_utf16() == 2)
        .map(|(i, _)| i)
        .collect()
}

impl Astral {
    fn unknown() -> Self {
        Self {
            unknown: true,
            by_text: HashMap::new(),
        }
    }

    /// Known to contain no astral characters at all.
    fn is_clean(&self) -> bool {
        !self.unknown && self.by_text.values().all(Vec::is_empty)
    }

    fn after_remote(&mut self, source_clean: bool) {
        if !(source_clean && self.is_clean()) {
            *self = Self::unknown();
        }
    }

    /// Astral positions for `text`, or `None` when it has none.
    fn lookup<'s, T: ReadTxn>(
        &'s self,
        text: &YText,
        txn: &T,
    ) -> Option<std::borrow::Cow<'s, [usize]>> {
        if let Some(positions) = self.by_text.get(&text.name) {
            return (!positions.is_empty()).then(|| positions.as_slice().into());
        }
        if !self.unknown {
            return None;
        }
        let positions = scan(&text.text.get_string(txn));
        (!positions.is_empty()).then(|| positions.into())
    }

    fn code_points<T: ReadTxn>(&self, text: &YText, txn: &T) -> usize {
        let units = text.text.len(txn) as usize;
        units - self.lookup(text, txn).map_or(0, |p| p.len())
    }

    fn utf16_read<T: ReadTxn>(&self, text: &YText, txn: &T, pos: usize) -> u32 {
        let before = self
            .lookup(text, txn)
            .map_or(0, |p| p.partition_point(|&i| i < pos));
        (pos + before) as u32
    }

    /// Convert a code-point splice to UTF-16 `(pos, delete)` and update the index.
    fn splice<T: ReadTxn>(
        &mut self,
        text: &YText,
        txn: &T,
        pos: usize,
        delete: usize,
        insert: &str,
    ) -> (u32, u32) {
        let inserted_astral = has_astral(insert);
        if !self.unknown && self.by_text.is_empty() && !inserted_astral {
            return (pos as u32, delete as u32);
        }
        if self.unknown && !self.by_text.contains_key(&text.name) {
            let positions = scan(&text.text.get_string(txn));
            self.by_text.insert(text.name.clone(), positions);
        }
        let positions = self.by_text.entry(text.name.clone()).or_default();
        let start = positions.partition_point(|&i| i < pos);
        let end = positions.partition_point(|&i| i < pos + delete);
        let result = ((pos + start) as u32, (delete + end - start) as u32);

        let inserted_len = insert.chars().count();
        let new_positions = scan(insert).into_iter().map(|i| i + pos);
        let tail: Vec<usize> = positions
            .drain(start..)
            .skip(end - start)
            .map(|i| i - delete + inserted_len)
            .collect();
        positions.extend(new_positions);
        positions.extend(tail);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_doc() -> (YrsDoc, YText) {
        let mut doc = YrsDoc::new();
        let text = doc.transact().create_text("t");
        (doc, text)
    }

    #[test]
    fn splices_by_code_point_with_astral_chars() {
        let (mut doc, text) = text_doc();
        doc.transact().splice_text(&text, 0, 0, "a😀b😀c");
        assert_eq!(doc.text_len(&text), 5);
        doc.transact().splice_text(&text, 3, 1, "XY");
        assert_eq!(doc.text(&text), "a😀bXYc");
        doc.transact().splice_text(&text, 1, 1, "");
        assert_eq!(doc.text(&text), "abXYc");
        doc.transact().splice_text(&text, 4, 0, "🎉");
        assert_eq!(doc.text(&text), "abXY🎉c");
        assert_eq!(doc.text_len(&text), 6);
    }

    #[test]
    fn loaded_docs_rescan_before_local_edits() {
        let (mut doc, text) = text_doc();
        doc.transact().splice_text(&text, 0, 0, "😀😀x");
        let mut loaded = YrsDoc::load(&doc.save());
        let text = loaded.get_text("t").unwrap();
        loaded.transact().splice_text(&text, 2, 1, "y");
        assert_eq!(loaded.text(&text), "😀😀y");
    }
}
