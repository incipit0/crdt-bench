//! Iterating every value of a map. `range_at` is in `not_ported` (ticket 2).

use std::hint::black_box;

use crate::api::Crdt;
use crate::battery::generators::string_map_doc;
use crate::battery::registry::{Benchmark, sampled};

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    for n in [10_000u64, 100_000] {
        out.push(sampled::<D>("range", format!("range/range/{n}"), move || {
            let doc = string_map_doc::<D>(n, |i| i.to_string(), |i| i.to_string());
            let root = doc.root_map();
            Box::new(move || {
                black_box(black_box(&doc).map_visit_values(&root));
            })
        }));
    }
}
