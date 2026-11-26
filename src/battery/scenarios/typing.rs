//! Single-character edits sent to a remote copy in different ways.

use crate::api::{Crdt, Feature, Txn};
use crate::battery::generators::{CONTENT, index, random_insert_text_doc, rng_with};
use crate::battery::registry::{Benchmark, gate, series};
use rand::rngs::StdRng;

const INITIAL_CHARS: u64 = 100_000;
const STEPS: usize = 1_000;
const BULK_STEPS: usize = 100;
const BULK_CHANGES: usize = 100;

/// A text document, a forked remote copy, and the RNG for edit positions.
fn setup<D: Crdt>(seed: u64) -> (D, D, D::Text, StdRng) {
    let mut rng = rng_with(seed);
    let mut doc: D = random_insert_text_doc(INITIAL_CHARS, 1, &mut rng);
    let remote = doc.fork();
    doc.save_incremental();
    let text = doc.get_text(CONTENT).unwrap();
    (doc, remote, text, rng)
}

fn pos(rng: &mut StdRng) -> usize {
    index(rng, INITIAL_CHARS as usize)
}

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    out.push(gate::<D>(&[Feature::SyncProtocol], series::<D>("typing", "typing/single_char_sync", STEPS, || {
        let (mut doc, mut remote, text, mut rng) = setup::<D>(2);
        let mut state = D::SyncState::default();
        let mut remote_state = D::SyncState::default();
        doc.sync(&mut state, &mut remote, &mut remote_state);
        Box::new(move |_| {
            doc.transact().splice_text(&text, pos(&mut rng), 0, ".");
            doc.sync(&mut state, &mut remote, &mut remote_state);
            assert!(doc.same_version(&mut remote));
        })
    })));

    out.push(series::<D>(
        "typing",
        "typing/single_char_apply_change",
        STEPS,
        || {
            let (mut doc, mut remote, text, mut rng) = setup::<D>(3);
            doc.record_local_changes();
            Box::new(move |_| {
                doc.transact().splice_text(&text, pos(&mut rng), 0, ".");
                let change = doc.last_local_change().unwrap();
                remote.apply_changes(vec![change]);
                assert!(doc.same_version(&mut remote));
            })
        },
    ));

    // In the original, the 100 edits accumulate in AutoCommit's implicit
    // transaction until `save_incremental` commits them, so they are one
    // change. Mirror that with one transaction.
    out.push(series::<D>(
        "typing",
        "typing/single_char_100_bulk_incremental_load",
        BULK_STEPS,
        || {
            let (mut doc, mut remote, text, mut rng) = setup::<D>(4);
            Box::new(move |_| {
                {
                    let mut tx = doc.transact();
                    for _ in 0..BULK_CHANGES {
                        tx.splice_text(&text, pos(&mut rng), 0, ".");
                    }
                }
                remote.load_incremental(&doc.save_incremental());
                assert!(doc.same_version(&mut remote));
            })
        },
    ));

    out.push(series::<D>(
        "typing",
        "typing/single_char_100_incremental_loads",
        BULK_STEPS,
        || {
            let (mut doc, mut remote, text, mut rng) = setup::<D>(5);
            Box::new(move |_| {
                for _ in 0..BULK_CHANGES {
                    doc.transact().splice_text(&text, pos(&mut rng), 0, ".");
                    remote.load_incremental(&doc.save_incremental());
                }
                assert!(doc.same_version(&mut remote));
            })
        },
    ));

    out.push(series::<D>("typing", "typing/single_char_merge", STEPS, || {
        let (mut doc, mut remote, text, mut rng) = setup::<D>(6);
        Box::new(move |_| {
            doc.transact().splice_text(&text, pos(&mut rng), 0, ".");
            remote.merge(&mut doc);
            assert!(doc.same_version(&mut remote));
        })
    }));
}
