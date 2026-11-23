//! Length of a text, list and map at the current version. The `_at`
//! variants are in `not_ported` (ticket 2).

use std::hint::black_box;

use crate::api::Crdt;
use crate::battery::generators::{CONTENT, list_splice_100, string_map_doc, text_splice_100};
use crate::battery::registry::{Benchmark, sampled};

const N: u64 = 100_000;

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    out.push(sampled::<D>("length", "length/text_len_now", || {
        let doc = text_splice_100::<D>(N);
        let text = doc.get_text(CONTENT).unwrap();
        Box::new(move || {
            black_box(doc.text_len(&text));
        })
    }));
    out.push(sampled::<D>("length", "length/list_len_now", || {
        let doc = list_splice_100::<D>(N);
        let list = doc.get_list(CONTENT).unwrap();
        Box::new(move || {
            black_box(doc.list_len(&list));
        })
    }));
    out.push(sampled::<D>("length", "length/map_len_now", || {
        let doc = string_map_doc::<D>(N, |i| format!("key{i}"), |i| format!("value{i}"));
        let root = doc.root_map();
        Box::new(move || {
            black_box(doc.map_len(&root));
        })
    }));
}
