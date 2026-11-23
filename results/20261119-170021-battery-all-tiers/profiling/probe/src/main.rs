//! Where does build_big_paste's time go? One 100k-char splice, timed phase by phase.
use automerge::{AutoCommit, ObjType, ROOT, ReadDoc, transaction::Transactable};
use rand::{Rng, SeedableRng, distr::Alphanumeric, rngs::StdRng};
use std::time::{Duration, Instant};
use yrs::{GetString, Text, Transact};

const N: usize = 100_000;
const REPS: usize = 30;

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn ms(d: Duration) -> String {
    format!("{:8.3} ms", d.as_secs_f64() * 1e3)
}

fn main() {
    let mut generate = vec![];
    let (mut am_new, mut am_splice, mut am_commit, mut am_drop, mut am_total) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut y_insert, mut y_drop, mut y_total) = (vec![], vec![], vec![]);
    for rep in 0..REPS {
        let t = Instant::now();
        let s: String = StdRng::seed_from_u64(rep as u64)
            .sample_iter(&Alphanumeric)
            .take(N)
            .map(char::from)
            .collect();
        generate.push(t.elapsed());

        // Automerge
        let t0 = Instant::now();
        let mut doc = AutoCommit::new();
        let text = doc.put_object(ROOT, "content", ObjType::Text).unwrap();
        let t1 = Instant::now();
        doc.splice_text(&text, 0, 0, &s).unwrap();
        let t2 = Instant::now();
        doc.commit();
        let t3 = Instant::now();
        assert_eq!(doc.length(&text), N);
        let t4 = Instant::now();
        drop(doc);
        let t5 = Instant::now();
        am_new.push(t1 - t0);
        am_splice.push(t2 - t1);
        am_commit.push(t3 - t2);
        am_drop.push(t5 - t4);
        am_total.push((t3 - t0) + (t5 - t4));

        // Yrs
        let t0 = Instant::now();
        let ydoc = yrs::Doc::new();
        let ytext = ydoc.get_or_insert_text("content");
        {
            let mut tx = ydoc.transact_mut();
            ytext.insert(&mut tx, 0, &s);
        } // commits here
        let t1 = Instant::now();
        assert_eq!(ytext.get_string(&ydoc.transact()).len(), N);
        let t2 = Instant::now();
        drop(ydoc);
        let t3 = Instant::now();
        y_insert.push(t1 - t0);
        y_drop.push(t3 - t2);
        y_total.push((t1 - t0) + (t3 - t2));
    }
    println!("generate 100k random chars       {}", ms(median(generate)));
    println!("automerge  new + create text     {}", ms(median(am_new)));
    println!("automerge  splice_text (100k)    {}", ms(median(am_splice)));
    println!("automerge  commit                {}", ms(median(am_commit)));
    println!("automerge  drop                  {}", ms(median(am_drop)));
    println!("automerge  total (excl. gen)     {}", ms(median(am_total)));
    println!("yrs        insert + commit       {}", ms(median(y_insert)));
    println!("yrs        drop                  {}", ms(median(y_drop)));
    println!("yrs        total (excl. gen)     {}", ms(median(y_total)));
}
