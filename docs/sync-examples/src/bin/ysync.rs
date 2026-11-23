use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, GetString, Options, ReadTxn, StateVector, Text, Transact, Update};

fn doc(id: u64) -> Doc { Doc::with_options(Options::with_client_id(ClientID::new(id))) }
fn show(name: &str, d: &Doc) {
    let t = d.get_or_insert_text("t");
    let txn = d.transact();
    println!("  {name}: text={:?}  state vector={:?}", t.get_string(&txn), txn.state_vector());
}
fn main() {
    let alice = doc(1); let bob = doc(2);
    let ta = alice.get_or_insert_text("t"); let tb = bob.get_or_insert_text("t");
    ta.insert(&mut alice.transact_mut(), 0, "Hello");
    // initial sync so both share "Hello"
    let u = alice.transact().encode_state_as_update_v1(&StateVector::default());
    bob.transact_mut().apply_update(Update::decode_v1(&u).unwrap()).unwrap();
    println!("1. after initial sync:"); show("alice", &alice); show("bob  ", &bob);

    // concurrent offline edits
    ta.insert(&mut alice.transact_mut(), 5, " world");
    tb.insert(&mut bob.transact_mut(), 0, ">> ");
    tb.remove_range(&mut bob.transact_mut(), 3, 1); // delete 'H'
    println!("2. after offline edits:"); show("alice", &alice); show("bob  ", &bob);

    // y-sync round
    let sv_a = alice.transact().state_vector().encode_v1();
    let sv_b = bob.transact().state_vector().encode_v1();
    println!("3. SyncStep1: alice sends SV ({} bytes) {:?}; bob sends SV ({} bytes) {:?}", sv_a.len(), StateVector::decode_v1(&sv_a).unwrap(), sv_b.len(), StateVector::decode_v1(&sv_b).unwrap());
    let for_bob = alice.transact().encode_diff_v1(&StateVector::decode_v1(&sv_b).unwrap());
    let for_alice = bob.transact().encode_diff_v1(&StateVector::decode_v1(&sv_a).unwrap());
    println!("4. SyncStep2: alice -> bob {} bytes: {:?}", for_bob.len(), Update::decode_v1(&for_bob).unwrap());
    println!("   SyncStep2: bob -> alice {} bytes: {:?}", for_alice.len(), Update::decode_v1(&for_alice).unwrap());
    bob.transact_mut().apply_update(Update::decode_v1(&for_bob).unwrap()).unwrap();
    alice.transact_mut().apply_update(Update::decode_v1(&for_alice).unwrap()).unwrap();
    println!("5. after applying:"); show("alice", &alice); show("bob  ", &bob);
    let again = alice.transact().encode_diff_v1(&bob.transact().state_vector());
    println!("6. syncing again: alice -> bob diff is {} bytes: {:?}", again.len(), Update::decode_v1(&again).unwrap());
}
