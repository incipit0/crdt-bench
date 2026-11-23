//! Map workloads at several sizes, each measured as build, save, load and
//! apply (replaying every change into an empty document).
//!
//! `repeated_increment` needs a counter type and is registered in
//! `not_ported` until ticket 2 decides how to handle it.

use std::hint::black_box;

use crate::api::{Crdt, Txn, Value};
use crate::battery::generators::deep_history_doc;
use crate::battery::registry::{Benchmark, rebuilt, sampled};

pub const SIZES: [u64; 3] = [100, 1_000, 10_000];

fn repeated_put<D: Crdt>(n: u64) -> D {
    put_all((0..n).map(|i| ("0".to_string(), i)))
}

fn increasing_put<D: Crdt>(n: u64) -> D {
    put_all((0..n).map(|i| (i.to_string(), i)))
}

fn decreasing_put<D: Crdt>(n: u64) -> D {
    put_all((0..n).rev().map(|i| (i.to_string(), i)))
}

/// All `(key, value)` puts in one transaction.
fn put_all<D: Crdt>(puts: impl Iterator<Item = (String, u64)>) -> D {
    let mut doc = D::new();
    let root = doc.root_map();
    {
        let mut tx = doc.transact();
        for (key, value) in puts {
            tx.map_put(&root, &key, Value::Int(value as i64));
        }
    }
    doc
}

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    let cases: [(&str, fn(u64) -> D); 3] = [
        ("repeated_put", repeated_put::<D>),
        ("increasing_put", increasing_put::<D>),
        ("decreasing_put", decreasing_put::<D>),
    ];
    for n in SIZES {
        for (operation, build) in cases {
            out.push(sampled::<D>("map", format!("map/{operation}/{n}"), move || {
                Box::new(move || {
                    black_box(build(n));
                })
            }));
            out.push(sampled::<D>("map", format!("map/save/{operation}/{n}"), move || {
                let mut doc = build(n);
                Box::new(move || {
                    black_box(doc.save());
                })
            }));
            out.push(sampled::<D>("map", format!("map/load/{operation}/{n}"), move || {
                let data = build(n).save();
                Box::new(move || {
                    black_box(D::load(&data));
                })
            }));
            out.push(rebuilt::<D, _, _, _, _, _>(
                "map",
                format!("map/apply/{operation}/{n}"),
                move || {
                    let changes = build(n).changes();
                    move || changes.clone()
                },
                |changes: Vec<D::Change>| {
                    let mut doc = D::new();
                    doc.apply_changes(changes);
                    doc
                },
            ));
        }
        out.push(sampled::<D>("map", format!("map/deep_history/{n}"), move || {
            Box::new(move || {
                black_box(deep_history_doc::<D>(n));
            })
        }));
    }
}
