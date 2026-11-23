//! Applying single-character edits to a remote copy, one step at a time.

use crate::api::{Crdt, Txn};
use crate::battery::generators::{CONTENT, index, random_insert_text_doc, rng_with};
use crate::battery::registry::{Benchmark, series};

const INITIAL_CHARS: u64 = 100_000;
const STEPS: usize = 1_000;

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    out.push(series::<D>(
        "apply",
        "apply/single_char_incremental_load",
        STEPS,
        || {
            let mut rng = rng_with(1);
            let mut doc: D = random_insert_text_doc(INITIAL_CHARS, 1, &mut rng);
            let mut remote = doc.fork();
            // Start incremental saves from the forked state, so the first
            // step sends one edit rather than the whole document.
            doc.save_incremental();
            let text = doc.get_text(CONTENT).unwrap();
            Box::new(move |_| {
                let pos = index(&mut rng, INITIAL_CHARS as usize);
                doc.transact().splice_text(&text, pos, 0, ".");
                remote.load_incremental(&doc.save_incremental());
                assert!(doc.same_version(&mut remote));
            })
        },
    ));
}
