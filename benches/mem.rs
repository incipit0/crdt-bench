use std::fs;
use std::hint::black_box;
use std::io::Write;

use crdt_bench::bam::BenchAM;
use crdt_bench::bench_utils::env_or;
use crdt_bench::byrs::BenchYrs;
use crdt_bench::crdt::{Crdt, CrdtLib};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Defaults; override with CRDT_LIB (automerge|yrs) and CRDT_FILE_SIZE.
/// If CRDT_MEM_OUT is set, a JSON line with the heap stats is appended to that file
/// (this is what scripts/bench.py reads).
static FILE_SZ: &str = "6k";

fn bench_mem(lib: CrdtLib, file_sz: &str) {
    let file_to_load = match lib {
        CrdtLib::Automerge => format!("./benches/data/rnd-{}.am", file_sz),
        CrdtLib::Yrs => format!("./benches/data/rnd-{}.yrs", file_sz),
    };

    let data = fs::read(file_to_load).unwrap();
    let doc: Box<dyn Crdt> = match lib {
        CrdtLib::Automerge => Box::new(BenchAM::default()),
        CrdtLib::Yrs => Box::new(BenchYrs::default()),
    };

    let doc = doc.load(&data);
    let stats = dhat::HeapStats::get();
    println!("current allocation {:?}", stats);

    if let Ok(out) = std::env::var("CRDT_MEM_OUT") {
        let line = format!(
            "{{\"lib\":\"{}\",\"file_size\":\"{}\",\"max_bytes\":{},\"curr_bytes\":{},\"max_blocks\":{},\"curr_blocks\":{},\"total_bytes\":{}}}\n",
            doc.borrow().name(),
            file_sz,
            stats.max_bytes,
            stats.curr_bytes,
            stats.max_blocks,
            stats.curr_blocks,
            stats.total_bytes
        );
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out)
            .and_then(|mut f| f.write_all(line.as_bytes()))
            .unwrap_or_else(|e| panic!("cannot write {out}: {e}"));
    }

    black_box(doc);
}

fn main() {
    #[cfg(not(feature = "dhat-heap"))]
    eprintln!("warning: built without --features dhat-heap, heap stats will be zero");

    let _profiler = dhat::Profiler::new_heap();
    let lib = match env_or("CRDT_LIB", String::from("yrs"))
        .to_lowercase()
        .as_str()
    {
        "automerge" | "am" => CrdtLib::Automerge,
        "yrs" | "yjs" => CrdtLib::Yrs,
        other => panic!("unknown CRDT_LIB {other:?} (expected automerge or yrs)"),
    };
    let file_sz = env_or("CRDT_FILE_SIZE", String::from(FILE_SZ));

    println!(
        "memory profiling for {:?} by loading file size {}",
        lib, file_sz
    );
    bench_mem(lib, &file_sz);
}
