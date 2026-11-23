//! Time to build each generator document from scratch.

use std::hint::black_box;

use crate::api::Crdt;
use crate::battery::generators::{
    big_paste_doc, deep_history_doc, maps_in_maps_doc, poorly_simulated_typing_doc,
    text_splice_100,
};
use crate::battery::registry::{Benchmark, sampled};

const N: u64 = 100_000;

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    let cases: [(&str, fn(u64) -> D); 5] = [
        ("build/build_typing", poorly_simulated_typing_doc::<D>),
        ("build/build_big_paste", big_paste_doc::<D>),
        ("build/build_text_splice_100", text_splice_100::<D>),
        ("build/build_maps_in_maps_doc", maps_in_maps_doc::<D>),
        ("build/build_deep_history_doc", deep_history_doc::<D>),
    ];
    for (name, build) in cases {
        out.push(sampled::<D>("build", name, move || {
            Box::new(move || {
                black_box(build(N));
            })
        }));
    }
}
