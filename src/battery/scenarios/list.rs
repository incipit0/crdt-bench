//! Cursor, set and insert at random indexes of a 100k list. The `_at`
//! variants (against historical versions) are in `not_ported` (ticket 2).

use std::hint::black_box;

use crate::api::{Crdt, Feature, Txn, Value};
use crate::battery::generators::{CONTENT, index, list_splice_100, rng_with};
use crate::battery::registry::{Benchmark, BenchmarkKind, requires, sampled};

const N: u64 = 100_000;

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    let len = N as usize;

    out.push(requires::<D>(
        &[Feature::Cursors],
        BenchmarkKind::Sampled,
        "list",
        "list/list_cursor_now",
        |base| {
            sampled::<D>("list", base, move || {
                let doc = list_splice_100::<D>(N);
                let list = doc.get_list(CONTENT).unwrap();
                let mut rng = rng_with(11);
                Box::new(move || {
                    let cursor = doc.list_cursor(&list, index(&mut rng, len));
                    black_box(doc.list_cursor_position(&list, &cursor));
                })
            })
        },
    ));

    out.push(sampled::<D>("list", "list/list_update_now", move || {
        let mut doc = list_splice_100::<D>(N);
        let list = doc.get_list(CONTENT).unwrap();
        let mut rng = rng_with(12);
        Box::new(move || {
            let pos = index(&mut rng, len);
            doc.transact().list_set(&list, pos, Value::Str("x"));
        })
    }));

    out.push(sampled::<D>("list", "list/list_splice_index_now", move || {
        let mut doc = list_splice_100::<D>(N);
        let list = doc.get_list(CONTENT).unwrap();
        let mut rng = rng_with(13);
        Box::new(move || {
            let pos = index(&mut rng, len);
            doc.transact().splice_list(&list, pos, 0, &[Value::Str("x")]);
        })
    }));
}
