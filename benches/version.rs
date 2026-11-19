use std::{any::Any, cell::RefCell, hint::black_box, rc::Rc};

use automerge::ChangeHash;
use crdt_bench::{
    bam::BenchAM,
    bench_utils::{self, am_doc, generate_random_string, yrs_doc, yrs_no_gc},
    byrs::BenchYrs,
    crdt::{Crdt, CrdtLib, DocRef},
};
use criterion::{Criterion, criterion_group, criterion_main};
use yrs::{
    Observable, ReadTxn, Snapshot, Subscription, Text, Update,
    types::{Delta, text::YChange},
    updates::{
        decoder::Decode,
        encoder::{Encoder, EncoderV1},
    },
};

static N: usize = 10;
static NUM_VERS: usize = 10000;
static SAMPLE_SIZE: usize = 10;

fn ver_to_diff() -> std::ops::Range<usize> {
    0..std::cmp::min(19, NUM_VERS - 1)
}

fn ver_to_back() -> std::ops::Range<usize> {
    0..NUM_VERS
}

enum Checkpoint {
    AM(Vec<ChangeHash>),
    Yrs(Snapshot),
}

fn checkpoint(doc: Rc<RefCell<dyn Any>>) -> Checkpoint {
    let mut doc = doc.borrow_mut();
    if let Some(doc) = doc.downcast_mut::<BenchAM>() {
        Checkpoint::AM(doc.get_heads())
    } else if let Some(doc) = doc.downcast_mut::<BenchYrs>() {
        Checkpoint::Yrs(doc.transact().snapshot())
    } else {
        unreachable!("unknown doc type");
    }
}

fn revert_version(doc: Rc<RefCell<dyn Any>>, checkpoints: &Vec<Checkpoint>, to_ver: usize) {
    let mut doc = doc.borrow_mut();
    if let Some(doc1) = doc.downcast_mut::<BenchYrs>() {
        let doc2 = yrs_doc();
        let doc2 = doc2.borrow_mut();
        match &checkpoints[to_ver] {
            Checkpoint::Yrs(snapshot) => {
                let txn = doc1.transact();
                let mut encoder = EncoderV1::new();
                txn.encode_state_from_snapshot(snapshot, &mut encoder)
                    .unwrap();
                let update = encoder.to_vec();
                {
                    let mut txn = doc2.transact_mut();
                    txn.apply_update(Update::decode_v1(&update).unwrap())
                        .unwrap();
                }
                debug_assert_eq!(doc2.text().len(), to_ver * N);
                black_box(doc2.text());
            }
            _ => unreachable!(""),
        }
        debug_assert_eq!(doc2.text().len(), to_ver * N);
    } else if let Some(doc) = doc.downcast_mut::<BenchAM>() {
        let snap = match &checkpoints[to_ver] {
            Checkpoint::AM(heads) => heads,
            _ => unreachable!(),
        };
        let text = doc.text_at(snap).unwrap();
        debug_assert_eq!(text.len(), to_ver * N);
        black_box(text);
    } else {
        unreachable!("unknown doc type");
    }
}

fn edit_and_go_back(c: &mut Criterion) {
    let docs: Vec<DocRef> = vec![am_doc(), yrs_no_gc()];
    let mut group = c.benchmark_group(format!(
        "Make changes {} character with {} versions and revert to previous {:?} versions",
        N,
        NUM_VERS,
        ver_to_back()
    ));
    group.sample_size(SAMPLE_SIZE);

    for doc in docs {
        let name: String = String::from(doc.borrow().name());

        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let new_doc = if matches!(doc.borrow().crdt_lib(), CrdtLib::Yrs) {
                        Rc::new(RefCell::new(BenchYrs::new_no_gc()))
                    } else {
                        doc.borrow().new()
                    };
                    let mut texts = Vec::new();
                    for _ in 0..NUM_VERS {
                        texts.push(generate_random_string(N));
                    }
                    (new_doc, texts)
                },
                |(doc, texts)| {
                    let mut checkpoints = vec![checkpoint(doc.clone())];
                    let mut ofs: usize = 0;
                    for (_i, t) in texts.iter().enumerate() {
                        bench_utils::insert_1b1(&t, ofs, doc.clone());
                        checkpoints.push(checkpoint(doc.clone()));
                        ofs += t.len();
                    }
                    for ver in ver_to_back() {
                        revert_version(doc.clone(), &checkpoints, ver);
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
}

fn track_version(
    doc: Rc<RefCell<dyn Any>>,
) -> (Rc<RefCell<Vec<Vec<Delta>>>>, Option<Subscription>) {
    // Collect updates as they happen
    let doc = doc.borrow();
    if let Some(doc) = doc.downcast_ref::<BenchYrs>() {
        let all_deltas = Rc::new(RefCell::new(Vec::new()));
        let all_deltas_clone = all_deltas.clone();
        let tref = doc.get_text_obj();
        let sub = tref.observe(move |txn, event| {
            all_deltas_clone
                .borrow_mut()
                .push(event.delta(txn).to_vec());
        });
        (all_deltas, Some(sub))
    } else {
        (Rc::new(RefCell::new(vec![])), None)
    }
}

fn edit_and_diff(c: &mut Criterion) {
    let mut texts = Vec::new();
    for _ in 0..NUM_VERS {
        texts.push(generate_random_string(N));
    }

    let am_setup = |doc: Rc<RefCell<BenchAM>>, texts: Vec<String>| {
        let mut ofs: usize = 0;
        let mut checkpoints = Vec::new();
        for t in &texts {
            bench_utils::insert_1b1(t, ofs, doc.clone());
            checkpoints.push(doc.borrow_mut().get_heads());
            ofs += t.len();
        }
        debug_assert_eq!(doc.borrow().text().len(), ofs);
        checkpoints
            .into_iter()
            .map(|heads| Checkpoint::AM(heads))
            .collect::<Vec<Checkpoint>>()
    };

    let am_diff = |(doc, checkpoints): (Rc<RefCell<BenchAM>>, Vec<Checkpoint>)| {
        let mut doc = doc.borrow_mut();
        // let tot = doc.encoded_state().iter().len();
        // println!("am doc size {}", tot);

        let heads = doc.get_heads();
        for v in ver_to_diff() {
            let Checkpoint::AM(ref cp) = checkpoints[v] else {
                unreachable!()
            };
            let patch = doc.diff(&cp, &heads);
            assert!(patch.len() > 0);
        }
    };

    // for yrs we take snapshots each time we finish adding a chunk of texts
    // more snapshots mean more space needed
    // Yrs provides two APIs: diff_range and delta. Diff returns a git diff like
    // description for two snapshots, which describes what has been added, removed, etc.
    // A delta is more like an automerge patch where describes a single change done
    // over the text
    let yrs_setup = |doc: Rc<RefCell<BenchYrs>>, texts: Vec<String>| {
        let mut checkpoints = Vec::new();
        let mut ofs = 0;
        checkpoints.push(doc.borrow().transact().snapshot());
        for t in &texts {
            bench_utils::insert_1b1(t, ofs, doc.clone());
            ofs += t.len();
            let doc = doc.borrow();
            let txn = doc.transact();
            checkpoints.push(txn.snapshot());
        }
        debug_assert_eq!(doc.borrow().text().len(), ofs);
        checkpoints
            .into_iter()
            .map(|snap| Checkpoint::Yrs(snap))
            .collect::<Vec<Checkpoint>>()
    };

    let yrs_diff = |(doc, checkpoints): (Rc<RefCell<BenchYrs>>, Vec<Checkpoint>)| {
        // let tot: usize = checkpoints.iter().map(|c| c.encode_v1().len()).sum();
        // let doc_size = doc.borrow_mut().encoded_state().len();
        // println!(
        //     "total checkpoints encoded size {}, doc size {}",
        //     tot, doc_size
        // );
        let last_snapshot = checkpoints.last().map(|cp| match cp {
            Checkpoint::Yrs(snap) => snap,
            _ => unreachable!(),
        });

        for v in ver_to_diff() {
            let doc_borrow = doc.borrow_mut();
            let tref = doc_borrow.get_text_obj();

            let Checkpoint::Yrs(ref cur_snapshot) = checkpoints[v] else {
                unreachable!()
            };

            let diff = tref.diff_range(
                &mut doc_borrow.transact_mut(),
                last_snapshot,
                Some(&cur_snapshot),
                YChange::identity,
            );
            assert!(diff.len() > 0);
        }
    };

    let yrs_delta = |(doc, texts): (Rc<RefCell<BenchYrs>>, Vec<String>)| {
        let (deltas, _sub) = track_version(doc.clone());
        let mut ofs = 0;
        for t in &texts {
            bench_utils::insert_1b1(t, ofs, doc.clone());
            ofs += t.len();
        }
        debug_assert_eq!(doc.borrow().text().len(), ofs);

        let mut diff = vec![];
        let deltas = deltas.borrow();
        for v in ver_to_diff() {
            diff.push(&deltas[v]);
            assert!(diff.len() > 0);
        }
    };

    let mut group = c.benchmark_group(format!(
        "Version control {NUM_VERS} versions each have {N} chars"
    ));
    group.sample_size(SAMPLE_SIZE);

    group.bench_function("Automerge", |b| {
        b.iter_batched(
            || {
                let new_doc = am_doc();
                let mut texts = Vec::new();
                for _ in 0..NUM_VERS {
                    texts.push(generate_random_string(N));
                }
                let checkpoints = am_setup(new_doc.clone(), texts);
                (new_doc.clone(), checkpoints)
            },
            am_diff,
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("Yrs", |b| {
        b.iter_batched(
            || {
                let new_doc = yrs_doc();
                let mut texts = Vec::new();
                for _ in 0..NUM_VERS {
                    texts.push(generate_random_string(N));
                }
                let chkpts = yrs_setup(new_doc.clone(), texts);
                (new_doc.clone(), chkpts)
            },
            yrs_diff,
            criterion::BatchSize::SmallInput,
        );
    });

    // The delta tracking is mainly used for applications to see what is happening
    // with each delta, e.g. what they need to do
    // group.bench_function("Yrs + delta tracking", |b| {
    //     b.iter_batched(
    //         || {
    //             let new_doc = yrs_doc();
    //             let mut texts = Vec::new();
    //             for _ in 0..NUM_VERS {
    //                 texts.push(generate_random_string(N));
    //             }
    //             (new_doc, texts)
    //         },
    //         yrs_delta,
    //         criterion::BatchSize::SmallInput,
    //     );
    // });
}

criterion_group!(version, edit_and_diff);

criterion_main!(version);
