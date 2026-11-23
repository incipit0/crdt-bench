//! Loading and saving generated documents.

use std::hint::black_box;

use crate::api::{Crdt, Txn, Value};
use crate::battery::generators::{
    big_paste_doc, big_random_doc, deep_history_doc, maps_in_maps_doc, poorly_simulated_typing_doc,
    text_splice_100,
};
use crate::battery::registry::{Benchmark, sampled};

const N: u64 = 100_000;
const ACTOR_SIZES: [u64; 3] = [10, 100, 1000];

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    // The `/1000` suffix on deep_history is kept from the original battery,
    // which also builds that document with N = 100,000.
    let cases: [(&str, fn(u64) -> D); 6] = [
        ("typing", poorly_simulated_typing_doc::<D>),
        ("big_paste", big_paste_doc::<D>),
        ("deep_history/1000", deep_history_doc::<D>),
        ("text_splice_100", text_splice_100::<D>),
        ("maps_in_maps", maps_in_maps_doc::<D>),
        ("big_random", big_random_doc::<D>),
    ];
    for (suffix, build) in cases {
        let (load, save) = suffix
            .split_once('/')
            .map_or((format!("load_{suffix}"), format!("save_{suffix}")), |(a, b)| {
                (format!("load_{a}/{b}"), format!("save_{a}/{b}"))
            });
        out.push(sampled::<D>("load_save", format!("load_save/{load}"), move || {
            let data = build(N).save();
            Box::new(move || {
                black_box(D::load(&data));
            })
        }));
        out.push(sampled::<D>("load_save", format!("load_save/{save}"), move || {
            let mut doc = build(N);
            Box::new(move || {
                black_box(doc.save());
            })
        }));
    }

    for n in ACTOR_SIZES {
        out.push(sampled::<D>(
            "load_save",
            format!("load_save/load_actors/load/{n}"),
            move || {
                let data = doc_with_agents::<D>(n);
                Box::new(move || {
                    black_box(D::load(&data));
                })
            },
        ));
    }
}

/// A document where each of `n` agents wrote one key.
fn doc_with_agents<D: Crdt>(n: u64) -> Vec<u8> {
    let mut doc = D::new();
    for i in 0..n {
        let mut author = D::with_agent(i);
        let root = author.root_map();
        author
            .transact()
            .map_put(&root, &format!("k{i}"), Value::Int(i as i64));
        doc.merge(&mut author);
    }
    doc.save()
}
