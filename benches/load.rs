use automerge::{Automerge, LoadOptions};
use crdt_bench::{
    bam::BenchAM,
    bench_utils::{all_docs, env_list, env_or, generate_random_string, insert_1b1},
    byrs::BenchYrs,
    crdt::{Crdt, CrdtLib},
};
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use std::{cell::RefCell, fs, hint::black_box, path::Path, rc::Rc};
use yrs::{Transact, updates::decoder::Decode};

/// Defaults; override with CRDT_SIZES (in-memory doc sizes), CRDT_FILE_SIZES
/// (which ./benches/data/rnd-<size>.* files to load) and CRDT_SAMPLE_SIZE.
static SIZES: &[usize] = &[6000];
static SAMPLE_SIZE: usize = 30;

static FILE_SIZES: &[&str] = &["6k"];
static FILENAME: &str = "./benches/data/rnd";

fn sample_size() -> usize {
    env_or("CRDT_SAMPLE_SIZE", SAMPLE_SIZE)
}

fn load(doc: Rc<RefCell<dyn Crdt>>, data: Vec<u8>) {
    let doc2 = doc.borrow().load(&data);
    debug_assert_eq!(doc.borrow().text(), doc2.borrow().text());
}

fn load_from_mem(c: &mut Criterion) {
    let doca = Rc::new(RefCell::new(BenchAM::default()));
    let docy = Rc::new(RefCell::new(BenchYrs::default()));
    let docs: Vec<Rc<RefCell<dyn Crdt>>> = vec![doca, docy];

    let mut group = c.benchmark_group("Time loading doc from memory");
    group.sample_size(sample_size());
    for n in env_list("CRDT_SIZES", SIZES) {
        for doc in &docs {
            let bench_name = String::from(doc.borrow().name());
            group.bench_function(BenchmarkId::new(bench_name, n), |b| {
                b.iter_batched(
                    || {
                        let doc = { doc.borrow().new() };
                        let text = generate_random_string(n);
                        insert_1b1(&text, 0, doc.clone());
                        let doc = doc.clone();
                        doc.borrow_mut().encoded_state()
                    },
                    |data| load(doc.clone(), data),
                    BatchSize::SmallInput,
                )
            });
        }
    }

    group.finish();
}

fn load_file(doc: Rc<RefCell<dyn Crdt>>, file_size: &str, check: bool) {
    let doc = doc.borrow_mut();
    let filename = format!("{}-{}", FILENAME, file_size);
    match doc.crdt_lib() {
        CrdtLib::Automerge => {
            let name = format!("{}.{}", filename, "am");
            let file_path = Path::new(&name);
            let opts = if check {
                LoadOptions::new().verification_mode(automerge::VerificationMode::Check)
            } else {
                LoadOptions::new().verification_mode(automerge::VerificationMode::DontCheck)
            };
            let data = fs::read(file_path).unwrap();
            let d = Automerge::load_with_options(&data, opts).unwrap();
            black_box(d);
        }
        CrdtLib::Yrs => {
            let name = format!("{}.{}", filename, "yrs");
            let file_path = Path::new(&name);
            let doc = yrs::Doc::new();
            let data = fs::read(file_path).unwrap();
            doc.transact_mut()
                .apply_update(yrs::Update::decode_v1(&data).unwrap())
                .unwrap();
            black_box(doc);
        }
    }
}

fn load_from_file(c: &mut Criterion) {
    let mut group = c.benchmark_group("Time loading doc from disk");
    group.sample_size(sample_size());
    let docs = all_docs();
    let file_sizes: Vec<String> = env_list(
        "CRDT_FILE_SIZES",
        &FILE_SIZES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    );
    for size in &file_sizes {
        for doc in &docs {
            let name = doc.borrow().name();
            group.bench_function(BenchmarkId::new(name, size), |b| {
                b.iter(|| load_file(doc.clone(), size, true))
            });
        }
    }
    group.finish();
}

criterion_group!(benches, load_from_file, load_from_mem);
criterion_main!(benches);
