fn main() {
    let p = std::env::args().nth(1).unwrap();
    let b = std::fs::read(&p).unwrap();
    for _ in 0..1 { std::hint::black_box(automerge::Automerge::load(&b).unwrap()); }
}
