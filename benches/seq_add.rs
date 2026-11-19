use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

use bench_utils::generate_random_string;
use crdt_bench::{
    bench_utils::{
        self, am_doc, env_list, env_or, insert_1b1, insert_1b1_with_updates, insert1big,
        insert1big_with_update,
    },
    crdt::Crdt,
};

/// Defaults; override at run time with CRDT_SAMPLE_SIZE / CRDT_SIZES (comma-separated).
static SAMPLE_SIZE: usize = 10;
static SIZES: &[usize] = &[60000];

fn sample_size() -> usize {
    env_or("CRDT_SAMPLE_SIZE", SAMPLE_SIZE)
}

fn sizes() -> Vec<usize> {
    env_list("CRDT_SIZES", SIZES)
}

/// Every (doc, size) combination, sizes in the outer loop.
fn with_sizes<D: Clone>(docs: &[D]) -> Vec<(D, usize)> {
    sizes()
        .into_iter()
        .flat_map(|n| docs.iter().map(move |d| (d.clone(), n)))
        .collect()
}

fn bench_1b1_insert_time(c: &mut Criterion) {
    let docs = bench_utils::all_docs();
    let mut group = c.benchmark_group("Insert characters one by one");
    group.sample_size(sample_size());
    for (doc, n) in with_sizes(&docs) {
        let bench_name = String::from(doc.borrow().name());
        group.bench_function(BenchmarkId::new(bench_name, n), |b| {
            b.iter_batched(
                // fresh doc per iteration (untimed), so every iteration starts empty
                || (doc.borrow().new(), generate_random_string(n)),
                // return the doc so dropping it happens outside the timed region
                |(doc, text)| {
                    insert_1b1(&text, 0, doc.clone());
                    doc
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_1b1_apply_time(c: &mut Criterion) {
    // let docs = bench_utils::all_docs();
    let docs = vec![am_doc()];
    let mut group = c.benchmark_group("Apply chars inserted one by one");
    group.sample_size(sample_size());
    for (doc1, n) in with_sizes(&docs) {
        let name: String = String::from(doc1.borrow().name());
        group.bench_function(BenchmarkId::new(name, n), |b| {
            b.iter_batched(
                || {
                    // fresh doc per iteration (untimed), so doc2 starts from an empty state
                    let doc1 = doc1.borrow().new();
                    let state = doc1.borrow_mut().encoded_state();
                    let doc2 = doc1.borrow().load(&state);
                    let t = generate_random_string(n);
                    let updates = insert_1b1_with_updates(&t, 0, doc1.clone()).concat();
                    (doc1, doc2, updates)
                },
                |(doc1, doc2, updates)| {
                    // for update in updates {
                    doc2.borrow_mut().apply_update(&updates);
                    // }
                    debug_assert_eq!(doc1.borrow().text(), doc2.borrow().text());
                    (doc1, doc2)
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn bench_bulk_insert_time(c: &mut Criterion) {
    let docs = bench_utils::all_docs();
    let mut group = c.benchmark_group("Append one big text");
    group.sample_size(sample_size());
    for (doc, n) in with_sizes(&docs) {
        let bench_name = String::from(doc.borrow().name());
        group.bench_function(BenchmarkId::new(bench_name, n), |b| {
            b.iter_batched(
                || (doc.borrow().new(), generate_random_string(n)),
                |(doc, text)| {
                    insert1big(&text, 0, doc.clone());
                    doc
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

fn bench_bulk_apply_time(c: &mut Criterion) {
    let docs = bench_utils::all_docs();
    let mut group = c.benchmark_group("Apply one big change");
    group.sample_size(sample_size());
    for (doc1, n) in with_sizes(&docs) {
        let name: String = String::from(doc1.borrow().name());
        group.bench_function(BenchmarkId::new(name, n), |b| {
            b.iter_batched(
                || {
                    let doc1 = doc1.borrow().new();
                    let state = doc1.borrow_mut().encoded_state();
                    let doc2 = { doc1.borrow().load(&state) };
                    let t = generate_random_string(n);

                    let update = insert1big_with_update(&t, 0, doc1.clone());
                    (doc1, doc2, update)
                },
                |(doc1, doc2, update)| {
                    doc2.borrow_mut().apply_update(&update);
                    debug_assert_eq!(doc1.borrow().text(), doc2.borrow().text());
                    (doc1, doc2)
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

criterion_group!(
    update_time,
    // bench_1b1_insert_time,
    bench_1b1_apply_time,
    // bench_bulk_insert_time,
    // bench_bulk_apply_time
);

criterion_main!(update_time);
