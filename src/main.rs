#![allow(unused)]

// use std::hint::black_box;

use std::{
    cell::RefCell,
    fs::{self, File},
    hint::black_box,
    io::Read,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};

use automerge::{AutoCommit, Automerge, ObjType, ReadDoc, transaction::Transactable};
use crdt_bench::{
    bam::BenchAM,
    bench_utils::{
        self, am_doc, generate_random_string, insert_1b1, insert_1b1_with_updates, yrs_doc,
    },
    byrs::BenchYrs,
    crdt::Crdt,
};
use json::object;
use tracing_subscriber::fmt::time;
use yrs::{
    Doc, GetString, Options, Update, WriteTxn, XmlFragment, XmlTextPrelim,
    types::{Attrs, Delta, text::YChange},
    updates::{
        decoder::Decode,
        encoder::{Encoder, EncoderV1},
    },
};
use yrs::{Observable, ReadTxn, Subscription, Text, Transact};

// fn load_file(check: bool) {
//     let filename = "./benches/moby-dick.amrg";
//     let data = std::fs::read(filename).unwrap();
//     let opts = if check {
//         LoadOptions::new().verification_mode(automerge::VerificationMode::Check)
//     } else {
//         LoadOptions::new().verification_mode(automerge::VerificationMode::DontCheck)
//     };
//     let result = Automerge::load_with_options(&data, opts).unwrap();
//     black_box(result);
// }

fn _f() {
    let mut doc = AutoCommit::new();
    let o = doc
        .put_object(automerge::ROOT, "test", automerge::ObjType::Text)
        .unwrap();
    doc.splice_text(o, 0, 0, "hi").unwrap();
    let update = doc.save_incremental();
    println!("update size {}", update.len());

    let mut doc2 = AutoCommit::new();
    doc2.load_incremental(&update).unwrap();
    let (_, p) = doc2.get(automerge::ROOT, "test").unwrap().unwrap();
    println!("doc2 {}", doc2.text(p).unwrap());
}

fn _g() {
    let d = Rc::new(RefCell::new(BenchYrs::default()));
    // bench_utils::insert_1b1(bench_utils::generate_random_string(N), d.clone());
    let data = d.borrow_mut().encoded_state();
    fs::write("benches/data/b1.yrs", &data).unwrap();
}

fn a() {
    let doc = Rc::new(RefCell::new(BenchAM::default()));
    let text = bench_utils::generate_random_string(6000);
    let text2 = bench_utils::generate_random_string(6000);

    let v1 = doc.borrow_mut().get_heads();
    bench_utils::insert_1b1(&text, 0, doc.clone());
    bench_utils::insert_1b1(&text2, 0, doc.clone());

    let mut doc = doc.borrow_mut();
    // let changes1 = doc.get_changes(&v1);

    let doc2 = doc.fork_at(&v1).unwrap();
    black_box(doc2);
}

fn y() {
    let doc = Rc::new(RefCell::new(BenchYrs::default()));

    // Collect updates as they happen
    let updates: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(vec![]));

    let updates_clone = updates.clone();
    let _sub = doc
        .borrow()
        .observe_update_v1(move |_, event| {
            updates_clone.borrow_mut().push(event.update.clone());
        })
        .unwrap();

    // Make edits (each generates an update)
    let text = generate_random_string(6000);
    let text2 = generate_random_string(6000);
    bench_utils::insert_1b1(&text, 0, doc.clone());
    bench_utils::insert_1b1(&text2, 0, doc.clone());

    let all_updates = updates.borrow();
    println!("Captured {} updates", all_updates.len());
}

fn _write() {
    let doc = Rc::new(RefCell::new(BenchAM::default()));
    let text = bench_utils::generate_random_string(6000);

    bench_utils::insert_1b1(&text, 0, doc.clone());

    let mut data = doc.borrow_mut().encoded_state();
    fs::write("test.am", data).unwrap();
}

fn load() {
    // tracing_subscriber::fmt::init();
    let data = fs::read("./benches/data/rnd-6k.am").unwrap();
    let mut doc2 = AutoCommit::load(&data).unwrap();
    // println!("change graph {:?}", doc2.change_graph());
    let o = doc2.get(automerge::ROOT, "btext").unwrap().unwrap().1;
    // println!("text {}", doc2.text(o).unwrap());
    println!("{:?}", doc2.dump());
    black_box(doc2);
}

fn _profil_apply_one_vs_apply_many() {
    let doc = am_doc();

    let mut ofs: usize = 0;
    // let mut checkpoints = Vec::new();

    let start = Instant::now();

    let texts = generate_random_string(100000);
    let updates = bench_utils::insert_1b1_with_updates(&texts, ofs, doc.clone()).concat();
    let mut doc2 = AutoCommit::new();

    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();

    doc2.load_incremental(&updates);
    println!("time taken {} ms", start.elapsed().as_millis());

    if let Ok(report) = guard.report().build() {
        let file = File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
    };
    drop(guard);
}

fn _check_previous_text_at() {
    let mut doc = AutoCommit::new();
    let t = doc
        .put_object(automerge::ROOT, "text", ObjType::Text)
        .unwrap();
    doc.splice_text(&t, 0, 0, "hello").unwrap();
    let head = doc.get_heads();
    doc.splice_text(&t, 5, 0, "world").unwrap();

    let t = doc.text_at(&t, &head).unwrap();

    println!("text was {}", t);
}

fn _yrs_snapshot_check() {
    let doc = Doc::with_options(Options {
        skip_gc: true, // in order to support revisions we cannot garbage collect deleted blocks
        ..Options::default()
    });
    let mut txn = doc.transact_mut();
    let f = txn.get_or_insert_xml_fragment("article");
    let text = f.insert(&mut txn, 0, XmlTextPrelim::new(""));

    const INIT: &str = "hello world";
    text.push(&mut txn, INIT);

    // save current state of the document: "hello world"
    let rev = txn.snapshot();

    // change document state
    let italic = Attrs::from([("i".into(), true.into())]);
    text.format(&mut txn, 6, 5, italic.clone());
    text.remove_range(&mut txn, 2, 3);
    assert_eq!(text.get_string(&txn), "he <i>world</i>");

    // encode past version of the document (new read txn still sees doc's full block store;
    // encode_state_from_snapshot encodes only the subset of that store described by rev)
    drop(txn);
    let mut encoder = EncoderV1::new();
    let txn = doc.transact();
    txn.encode_state_from_snapshot(&rev, &mut encoder).unwrap();
    let update = encoder.to_vec();

    // restore the past state
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let f = txn.get_or_insert_xml_fragment("article");
    txn.apply_update(Update::decode_v1(&update).unwrap());
    let text = f.get(&mut txn, 0).unwrap().into_xml_text().unwrap();

    println!("{}", text.get_string(&txn));
    assert_eq!(text.get_string(&txn), INIT);
}

static N: usize = 1;
static NUM_VERS: usize = 10000;

fn ver_to_diff() -> impl Iterator<Item = usize> {
    0..std::cmp::min(19, NUM_VERS - 1)
}

fn back_to_previous() {}

fn _individual_update_apply_time() {

    let mut doc = am_doc();
    let text = generate_random_string(1000);
    let mut times_mus = Vec::new();
    let updates = insert_1b1_with_updates(&text, 0, doc.clone());
    let doc2 = am_doc();
    for u in updates {
        let t = Instant::now();
        doc2.borrow_mut().apply_update(&u);
        let taken = t.elapsed().as_micros();
        times_mus.push(taken);
    }

    let tot: u128 = times_mus.iter().sum();
    println!(
        "single character applicaiton time (micro sec) {:?}, total {} (ms)",
        times_mus, tot / 1000
    );
    black_box(doc2);
}

fn main() {
    // let mut doc = AutoCommit::new();
    // let o = doc
    //     .put_object(automerge::ROOT, "text", ObjType::Text)
    //     .unwrap();
    // let text = generate_random_string(1000);
    // let head = doc.get_heads();
    // for s in text.chars() {
    //     doc.splice_text(&o, 0, 0, &s.to_string()).unwrap();
    // }
    // let chgs = doc.get_changes(&head);
    // println!("{:?}", chgs.len());

    // const NUM_VERS: usize = 5;
    // const N: usize = 10;

    // let doc = yrs_doc();
    // let all_deltas: Rc<RefCell<Vec<Vec<Delta>>>> = Rc::new(RefCell::new(Vec::new()));
    // let all_deltas_clone = all_deltas.clone();

    // let guard = doc.borrow();
    // let tref = guard.get_text_obj();
    // let _sub: Subscription = tref.observe(move |txn, event| {
    //     all_deltas_clone
    //         .borrow_mut()
    //         .push(event.delta(txn).to_vec());
    // });
    // drop(guard);

    // let mut texts = Vec::new();
    // for _ in 0..NUM_VERS {
    //     texts.push(generate_random_string(N));
    // }

    // let mut ofs = 0;
    // for t in &texts {
    //     insert_1b1(t, ofs, doc.clone());
    //     ofs += t.len();
    // }

    // println!("Total text len: {}", ofs);
    // let deltas = all_deltas.borrow();
    // println!("Delta batches (one per transaction): {}", deltas.len());

    // for (batch_i, batch) in deltas.iter().enumerate() {
    //     println!("\n--- Delta batch {} ({} ops) ---", batch_i, batch.len());
    //     for (i, op) in batch.iter().enumerate() {
    //         println!("  op {}: {:?}", i, op);
    //     }
    // }

    // for _ in 0..NUM_VERS {
    //     texts.push(generate_random_string(N));
    // }

    // println!("change graph {:?}", doc2.change_graph());
    // checkpoints.push(doc.borrow_mut().get_heads());
    // ofs += texts.len();

    // println!("time it takes to apply {}", start.elapsed().as_millis());
    // let mid = Instant::now();

    // debug_assert_eq!(doc.borrow().text().len(), ofs);
    // let mut doc = doc.borrow_mut();
    // // let tot = doc.encoded_state().iter().len();
    // // println!("am doc size {}", tot);
    // let heads = doc.get_heads();
    // for v in ver_to_diff() {
    //     let patch = doc.diff(&checkpoints[v], &heads);
    //     assert!(patch.len() > 0);
    // }

    // println!("time taken to diff {:?}", mid.elapsed().as_millis());

    // let s =
    //     fs::read_to_string("benches/data/editing-traces/sequential_traces/automerge-paper.json")
    //         .unwrap();
    // let parsed = json::parse(&s).unwrap();
    // for txn in parsed["txns"].members() {
    //     println!("{}", txn);
    // }
    // println!("{}", parsed["txns"]);


    // let mut doc = AutoCommit::new();
    // let data = fs::read("benches/data/rnd-600k.am").unwrap();
    // doc.load_incremental(&data);
    // println!("doc change graph {:?}", doc.change_graph());


}
