use automerge::Automerge;
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    for t in ["S1","S2","S3","A1","A2","C1","C2"] {
        let b = std::fs::read(format!("{dir}/{t}.automerge")).unwrap();
        let d = Automerge::load(&b).unwrap();
        let ch = d.get_changes(&[]);
        let raw: usize = ch.iter().map(|c| c.raw_bytes().len()).sum();
        let ops: usize = ch.iter().map(|c| c.len()).sum();
        println!("{t}: file={:>8} B  changes={:>6}  sum(change bytes)={:>9} B  ops={}", b.len(), ch.len(), raw, ops);
    }
}
