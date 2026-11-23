use automerge::sync::{self, SyncDoc};
use automerge::{transaction::Transactable, ActorId, AutoCommit, ChangeHash, ObjType, ReadDoc, ROOT};
use std::collections::HashMap;

fn lbl(h: &ChangeHash, names: &HashMap<ChangeHash, &str>) -> String {
    names.get(h).map(|s| s.to_string()).unwrap_or_else(|| format!("{}", &h.to_string()[..8]))
}
fn hs(v: &[ChangeHash], n: &HashMap<ChangeHash, &str>) -> String {
    format!("[{}]", v.iter().map(|h| lbl(h, n)).collect::<Vec<_>>().join(", "))
}
fn show(who: &str, d: &mut AutoCommit, n: &HashMap<ChangeHash, &str>) {
    let (_, t) = d.get(ROOT, "t").unwrap().unwrap();
    println!("  {who}: text={:?} heads={}", d.text(&t).unwrap(), hs(&d.get_heads(), n));
}
/// Run the sync loop, printing every message. Returns number of messages.
fn sync(a: &mut AutoCommit, sa: &mut sync::State, b: &mut AutoCommit, sb: &mut sync::State, n: &HashMap<ChangeHash, &str>) {
    let mut round = 0;
    loop {
        let mut progressed = false;
        for dir in 0..2 {
            let (from, to, fs, ts, name) = if dir == 0 { (&mut *a, &mut *b, &mut *sa, &mut *sb, "alice -> bob  ") } else { (&mut *b, &mut *a, &mut *sb, &mut *sa, "bob   -> alice") };
            if let Some(msg) = from.sync().generate_sync_message(fs) {
                round += 1;
                let bytes = msg.clone().encode();
                let have: Vec<String> = msg.have.iter().map(|h| format!("{{last_sync: {}, bloom: {} B}}", hs(&h.last_sync, n), h.bloom.to_bytes().len())).collect();
                let chunks: Vec<usize> = msg.changes.iter().map(|c| c.len()).collect();
                println!("  msg {round} {name} ({:>3} B): heads={} need={} have=[{}] changes={} chunk(s) {:?} B",
                    bytes.len(), hs(&msg.heads, n), hs(&msg.need, n), have.join(", "), chunks.len(), chunks);
                to.sync().receive_sync_message(ts, sync::Message::decode(&bytes).unwrap()).unwrap();
                progressed = true;
            }
        }
        if !progressed { break; }
    }
    println!("  (no more messages; {round} sent)");
}
fn main() {
    let mut names: HashMap<ChangeHash, &str> = HashMap::new();
    let mut alice = AutoCommit::new().with_actor(ActorId::from(vec![0xa1; 4]));
    let mut bob = AutoCommit::new().with_actor(ActorId::from(vec![0xb0; 4]));
    let (mut sa, mut sb) = (sync::State::new(), sync::State::new());

    let t = alice.put_object(ROOT, "t", ObjType::Text).unwrap();
    alice.splice_text(&t, 0, 0, "Hello").unwrap();
    alice.commit(); names.insert(alice.get_heads()[0], "A0");
    println!("1. initial sync (alice has A0 = create text + \"Hello\"):");
    sync(&mut alice, &mut sa, &mut bob, &mut sb, &names);
    show("alice", &mut alice, &names); show("bob  ", &mut bob, &names);

    alice.splice_text(&t, 5, 0, " world").unwrap(); alice.commit(); names.insert(alice.get_heads()[0], "A1");
    let (_, tb) = bob.get(ROOT, "t").unwrap().unwrap();
    bob.splice_text(&tb, 0, 0, ">> ").unwrap(); bob.commit(); names.insert(bob.get_heads()[0], "B1");
    bob.splice_text(&tb, 3, 1, "").unwrap(); bob.commit(); names.insert(bob.get_heads()[0], "B2");
    println!("2. after offline edits (alice: A1 = \" world\"; bob: B1 = \">> \", B2 = delete 'H'):");
    show("alice", &mut alice, &names); show("bob  ", &mut bob, &names);

    println!("3. sync, reusing each side's sync state from step 1:");
    sync(&mut alice, &mut sa, &mut bob, &mut sb, &names);
    show("alice", &mut alice, &names); show("bob  ", &mut bob, &names);

    println!("4. sync again with nothing new:");
    sync(&mut alice, &mut sa, &mut bob, &mut sb, &names);
}
