//! Syncing two peers with each library's sync protocol.
//!
//! Differences from the original battery: the source document is duplicated
//! outside the timer (the original clones it inside `full_many_tx`), and
//! random positions come from a seeded RNG.

use crate::api::{Crdt, Txn, Value};
use crate::battery::generators::{CONTENT, index, list_splice_100, rng_with, text_splice_100};
use crate::battery::registry::{Benchmark, rebuilt, sampled, series};

const FULL_SYNC_SIZE: u64 = 10_000;
const TINY_SYNC_INITIAL_SIZE: u64 = 100_000;
const TINY_SYNC_STEPS: usize = 1_000;

struct Peer<D: Crdt> {
    doc: D,
    state: D::SyncState,
}

impl<D: Crdt> Peer<D> {
    fn new(doc: D) -> Self {
        Self {
            doc,
            state: D::SyncState::default(),
        }
    }

    fn sync(&mut self, other: &mut Self) {
        self.doc
            .sync(&mut self.state, &mut other.doc, &mut other.state);
    }
}

fn increasing_put<D: Crdt>(n: u64, one_tx: bool) -> D {
    let mut doc = D::new();
    let root = doc.root_map();
    if one_tx {
        let mut tx = doc.transact();
        for i in 0..n {
            tx.map_put(&root, &i.to_string(), Value::Int(i as i64));
        }
    } else {
        for i in 0..n {
            doc.transact()
                .map_put(&root, &i.to_string(), Value::Int(i as i64));
        }
    }
    doc
}

fn full_sync<D: Crdt>(base: &'static str, n: u64, one_tx: bool) -> Benchmark {
    rebuilt::<D, _, _, _, _, _>(
        "sync",
        base,
        move || {
            let source = increasing_put::<D>(n, one_tx);
            move || (Peer::new(source.duplicate()), Peer::new(D::new()))
        },
        |(mut a, mut b): (Peer<D>, Peer<D>)| {
            a.sync(&mut b);
            (a, b)
        },
    )
}

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    out.push(full_sync::<D>("sync/full_many_tx", FULL_SYNC_SIZE, false));
    out.push(full_sync::<D>("sync/full_one_tx/100", 100, true));
    out.push(full_sync::<D>("sync/full_one_tx/1000", 1_000, true));
    // Named without a size in the original battery.
    out.push(full_sync::<D>("sync/full_one_tx", FULL_SYNC_SIZE, true));

    for n in [100u64, 1_000, 10_000] {
        out.push(sampled::<D>("sync", format!("sync/every_change/{n}"), move || {
            Box::new(move || {
                let mut a = Peer::new(D::new());
                let mut b = Peer::new(D::new());
                let root = a.doc.root_map();
                for i in 0..n {
                    a.doc
                        .transact()
                        .map_put(&root, &i.to_string(), Value::Int(i as i64));
                    a.sync(&mut b);
                }
            })
        }));
    }

    out.push(series::<D>("sync", "sync/tiny_text", TINY_SYNC_STEPS, || {
        let mut a = Peer::new(text_splice_100::<D>(TINY_SYNC_INITIAL_SIZE));
        let mut b = Peer::new(D::new());
        a.sync(&mut b);
        let text = a.doc.get_text(CONTENT).unwrap();
        let mut rng = rng_with(7);
        Box::new(move |_| {
            let pos = index(&mut rng, TINY_SYNC_INITIAL_SIZE as usize);
            a.doc.transact().splice_text(&text, pos, 1, "_");
            a.sync(&mut b);
        })
    }));

    out.push(series::<D>("sync", "sync/tiny_list", TINY_SYNC_STEPS, || {
        let mut a = Peer::new(list_splice_100::<D>(TINY_SYNC_INITIAL_SIZE));
        let mut b = Peer::new(D::new());
        a.sync(&mut b);
        let list = a.doc.get_list(CONTENT).unwrap();
        let mut rng = rng_with(8);
        Box::new(move |_| {
            let pos = index(&mut rng, TINY_SYNC_INITIAL_SIZE as usize);
            a.doc
                .transact()
                .splice_list(&list, pos, 0, &[Value::Str("_")]);
            a.sync(&mut b);
        })
    }));
}
