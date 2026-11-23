//! 100k characters stored as one map value vs as collaborative text, in both libraries.
//! Each iteration: new document, one edit in one transaction, commit, drop.
use automerge::{AutoCommit, ObjType, ROOT, transaction::Transactable};
use rand::{Rng, SeedableRng, distr::Alphanumeric, rngs::StdRng};
use std::time::{Duration, Instant};
use yrs::{Map, Text, Transact};

const N: usize = 100_000;
const REPS: usize = 101;

fn time(mut f: impl FnMut()) -> (f64, f64, f64) {
    for _ in 0..5 {
        f(); // warm up
    }
    let mut v: Vec<Duration> = (0..REPS)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect();
    v.sort();
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    (us(v[REPS / 2]), us(v[REPS / 10]), us(v[REPS * 9 / 10]))
}

fn main() {
    let s: String = StdRng::seed_from_u64(1).sample_iter(&Alphanumeric).take(N).map(char::from).collect();
    let cases: [(&str, Box<dyn FnMut()>); 4] = [
        ("automerge: map value (put)", Box::new(|| {
            let mut doc = AutoCommit::new();
            doc.put(ROOT, "value", s.as_str()).unwrap();
            doc.commit();
        })),
        ("yrs: map value (Map::insert)", Box::new(|| {
            let doc = yrs::Doc::new();
            let map = doc.get_or_insert_map("root");
            let mut tx = doc.transact_mut();
            map.insert(&mut tx, "value", s.as_str());
        })),
        ("yrs: text (Text::insert)", Box::new(|| {
            let doc = yrs::Doc::new();
            let text = doc.get_or_insert_text("content");
            let mut tx = doc.transact_mut();
            text.insert(&mut tx, 0, &s);
        })),
        ("automerge: text (splice_text)", Box::new(|| {
            let mut doc = AutoCommit::new();
            let text = doc.put_object(ROOT, "content", ObjType::Text).unwrap();
            doc.splice_text(&text, 0, 0, &s).unwrap();
            doc.commit();
        })),
    ];
    println!("{:<32} {:>11} {:>11} {:>11}", "100k chars, new doc + edit + commit + drop", "median µs", "p10 µs", "p90 µs");
    for (name, mut f) in cases {
        let (m, lo, hi) = time(&mut f);
        println!("{name:<32} {m:>11.1} {lo:>11.1} {hi:>11.1}");
    }
}
