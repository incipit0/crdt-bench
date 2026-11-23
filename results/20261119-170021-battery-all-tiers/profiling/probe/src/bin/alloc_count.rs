//! Heap allocations made by one 100k-char paste, per phase, in each library.
use automerge::{AutoCommit, ObjType, ROOT, transaction::Transactable};
use rand::{Rng, SeedableRng, distr::Alphanumeric, rngs::StdRng};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use yrs::{Text, Transact};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size(), Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new, Relaxed);
        unsafe { System.realloc(ptr, layout, new) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn phase(name: &str, f: impl FnOnce()) {
    let (a0, b0) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
    f();
    let (a, b) = (ALLOCS.load(Relaxed) - a0, BYTES.load(Relaxed) - b0);
    println!("{name:<28} {a:>9} allocations {:>10.1} MiB requested", b as f64 / 1048576.0);
}

fn main() {
    let s: String = StdRng::seed_from_u64(1).sample_iter(&Alphanumeric).take(100_000).map(char::from).collect();

    let mut doc = AutoCommit::new();
    let obj = doc.put_object(ROOT, "content", ObjType::Text).unwrap();
    doc.commit();
    phase("automerge splice_text", || doc.splice_text(&obj, 0, 0, &s).unwrap());
    phase("automerge commit", || {
        doc.commit();
    });

    let ydoc = yrs::Doc::new();
    let text = ydoc.get_or_insert_text("content");
    phase("yrs insert + commit", || {
        let mut tx = ydoc.transact_mut();
        text.insert(&mut tx, 0, &s);
    });
}
