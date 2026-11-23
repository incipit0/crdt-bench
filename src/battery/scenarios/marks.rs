//! Formatting marks on a 100k text.
//!
//! Automerge marks use `ExpandMark::Both`. Yrs formatting attributes have no
//! expand setting; text inserted at a mark's edge takes the formatting of the
//! character before it. `get_marks_at_end` has no point query in Yrs, so the
//! adapter walks the text's delta.

use std::cmp::{max, min};
use std::hint::black_box;

use rand::rngs::StdRng;

use crate::api::{Crdt, Feature, Txn};
use crate::battery::generators::{CONTENT, index, rng_with, text_splice_100};
use crate::battery::registry::{Benchmark, BenchmarkKind, rebuilt, requires, sampled};

const N: u64 = 100_000;

fn add_random_marks<D: Crdt>(doc: &mut D, rng: &mut StdRng, count: usize) {
    let text = doc.get_text(CONTENT).unwrap();
    let mut tx = doc.transact();
    let len = tx.text_len(&text);
    for i in 0..count {
        let a = index(rng, len);
        let b = index(rng, len);
        tx.mark(&text, min(a, b), max(a, b), &format!("mark{i}"));
    }
}

fn doc_with_marks<D: Crdt>(count: usize) -> D {
    let mut doc = text_splice_100::<D>(N);
    add_random_marks(&mut doc, &mut rng_with(21), count);
    doc
}

/// 1000 transactions, each splicing "XXX" at a random position.
fn splice_benchmark<D: Crdt>(base: &'static str, marks: usize, delete: usize) -> Benchmark {
    requires::<D>(
        &[Feature::Marks],
        BenchmarkKind::Sampled,
        "marks",
        base,
        |base| {
            rebuilt::<D, _, _, _, _, _>(
                "marks",
                base,
                move || {
                    let source = doc_with_marks::<D>(marks);
                    move || source.duplicate()
                },
                {
                    let mut rng = rng_with(22);
                    move |mut doc: D| {
                        let text = doc.get_text(CONTENT).unwrap();
                        for _ in 0..1000 {
                            let mut tx = doc.transact();
                            let pos = index(&mut rng, tx.text_len(&text));
                            tx.splice_text(&text, pos, delete, "XXX");
                        }
                        doc
                    }
                },
            )
        },
    )
}

pub fn register<D: Crdt>(out: &mut Vec<Benchmark>) {
    out.push(requires::<D>(
        &[Feature::Marks],
        BenchmarkKind::Sampled,
        "marks",
        "marks/add_mark",
        |base| {
            sampled::<D>("marks", base, || {
                let mut doc = text_splice_100::<D>(N);
                let mut rng = rng_with(23);
                Box::new(move || add_random_marks(&mut doc, &mut rng, 1))
            })
        },
    ));
    out.push(requires::<D>(
        &[Feature::Marks],
        BenchmarkKind::Sampled,
        "marks",
        "marks/get_marks_at_end",
        |base| {
            sampled::<D>("marks", base, || {
                let doc = doc_with_marks::<D>(10);
                let text = doc.get_text(CONTENT).unwrap();
                let index = doc.text_len(&text) - 1;
                Box::new(move || {
                    black_box(doc.marks_at(&text, index));
                })
            })
        },
    ));
    // As in the original, the run with marks inserts and the run without
    // also deletes one character per splice.
    out.push(splice_benchmark::<D>("marks/splice_with_marks", 1_000, 0));
    out.push(splice_benchmark::<D>("marks/splice_without_marks", 0, 1));
}
