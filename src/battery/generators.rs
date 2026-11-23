//! Document generators, ported from the Automerge battery's `src/lib.rs`.
//!
//! Unlike the original, every generator uses a seeded RNG, so each library
//! receives exactly the same sequence of edits and repeated runs build the
//! same documents.

use rand::distr::Alphanumeric;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::api::{Crdt, Txn, Value};

/// Root key used for the text or list object in generated documents.
pub const CONTENT: &str = "content";

const SEED: u64 = 0x5eed;

pub fn rng() -> StdRng {
    StdRng::seed_from_u64(SEED)
}

pub fn rng_with(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

pub fn random_string(rng: &mut StdRng, n: usize) -> String {
    rng.sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// A uniformly random index in `0..len`.
pub fn index(rng: &mut StdRng, len: usize) -> usize {
    rng.random_range(0..len)
}

/// `n` maps, each nested in the previous one, in one transaction.
#[inline(never)]
pub fn maps_in_maps_doc<D: Crdt>(n: u64) -> D {
    let mut doc = D::new();
    let mut map = doc.root_map();
    {
        let mut tx = doc.transact();
        for i in 0..n {
            map = tx.map_put_map(&map, &i.to_string());
        }
    }
    doc
}

/// One splice of `n` characters, in one transaction.
#[inline(never)]
pub fn big_paste_doc<D: Crdt>(n: u64) -> D {
    let mut rng = rng();
    let mut doc = D::new();
    {
        let mut tx = doc.transact();
        let text = tx.create_text(CONTENT);
        tx.splice_text(&text, 0, 0, &random_string(&mut rng, n as usize));
    }
    doc
}

/// `n` single characters at random positions, in one transaction.
#[inline(never)]
pub fn big_random_doc<D: Crdt>(n: u64) -> D {
    let mut rng = rng();
    let mut doc = D::new();
    {
        let mut tx = doc.transact();
        let text = tx.create_text(CONTENT);
        for len in 0..n as usize {
            let pos = index(&mut rng, len + 1);
            tx.splice_text(&text, pos, 0, &random_string(&mut rng, 1));
        }
    }
    doc
}

/// `n / 100` splices of 100 characters, in one transaction.
///
/// As in the original, the insert position is drawn from `0..=splices so far`
/// rather than `0..=characters so far`, so inserts cluster near the start.
#[inline(never)]
pub fn text_splice_100<D: Crdt>(n: u64) -> D {
    let mut rng = rng();
    let mut doc = D::new();
    {
        let mut tx = doc.transact();
        let text = tx.create_text(CONTENT);
        for len in 0..(n / 100) as usize {
            let pos = index(&mut rng, len + 1);
            tx.splice_text(&text, pos, 0, &random_string(&mut rng, 100));
        }
    }
    doc
}

/// Like [`text_splice_100`], but into a list of one-character strings.
#[inline(never)]
pub fn list_splice_100<D: Crdt>(n: u64) -> D {
    let mut rng = rng();
    let mut doc = D::new();
    {
        let mut tx = doc.transact();
        let list = tx.create_list(CONTENT);
        for len in 0..(n / 100) as usize {
            let pos = index(&mut rng, len + 1);
            let string = random_string(&mut rng, 100);
            // Alphanumeric, so every character is one byte.
            let letters: Vec<Value<'_>> = (0..string.len())
                .map(|i| Value::Str(&string[i..i + 1]))
                .collect();
            tx.splice_list(&list, pos, 0, &letters);
        }
    }
    doc
}

/// `n` single-character appends, one transaction each.
#[inline(never)]
pub fn poorly_simulated_typing_doc<D: Crdt>(n: u64) -> D {
    let mut rng = rng();
    let mut doc = D::new();
    let text = doc.transact().create_text(CONTENT);
    for pos in 0..n as usize {
        doc.transact()
            .splice_text(&text, pos, 0, &random_string(&mut rng, 1));
    }
    doc
}

/// `n` transactions, each overwriting root keys `x` and `y`.
#[inline(never)]
pub fn deep_history_doc<D: Crdt>(n: u64) -> D {
    let mut doc = D::new();
    let root = doc.root_map();
    for i in 0..n {
        let value = i.to_string();
        let mut tx = doc.transact();
        tx.map_put(&root, "x", Value::Str(&value));
        tx.map_put(&root, "y", Value::Str(&value));
    }
    doc
}

/// A text of `n` characters built by inserting `chunk`-sized random strings at
/// random positions, in one transaction. Used by the `apply` and `typing`
/// groups, which pass in their own seeded RNG.
pub fn random_insert_text_doc<D: Crdt>(n: u64, chunk: usize, rng: &mut StdRng) -> D {
    let mut doc = D::new();
    {
        let mut tx = doc.transact();
        let text = tx.create_text(CONTENT);
        tx.splice_text(&text, 0, 0, &random_string(rng, chunk));
        let mut len = chunk;
        for _ in 0..(n as usize / chunk) {
            let pos = index(rng, len);
            tx.splice_text(&text, pos, 0, &random_string(rng, chunk));
            len += chunk;
        }
    }
    doc
}

/// A root map with `n` string keys and values, in one transaction.
pub fn string_map_doc<D: Crdt>(n: u64, key: impl Fn(u64) -> String, value: impl Fn(u64) -> String) -> D {
    let mut doc = D::new();
    let root = doc.root_map();
    {
        let mut tx = doc.transact();
        for i in 0..n {
            tx.map_put(&root, &key(i), Value::Str(&value(i)));
        }
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{AmDoc, YrsDoc};

    fn text_of<D: Crdt>(doc: &D) -> String {
        doc.text(&doc.get_text(CONTENT).unwrap())
    }

    #[test]
    fn text_generators_match_across_libraries() {
        assert_eq!(
            text_of(&big_random_doc::<AmDoc>(2_000)),
            text_of(&big_random_doc::<YrsDoc>(2_000))
        );
        assert_eq!(
            text_of(&text_splice_100::<AmDoc>(10_000)),
            text_of(&text_splice_100::<YrsDoc>(10_000))
        );
        assert_eq!(
            text_of(&poorly_simulated_typing_doc::<AmDoc>(500)),
            text_of(&poorly_simulated_typing_doc::<YrsDoc>(500))
        );
        let am = random_insert_text_doc::<AmDoc>(1_000, 1, &mut rng());
        let yrs = random_insert_text_doc::<YrsDoc>(1_000, 1, &mut rng());
        assert_eq!(text_of(&am), text_of(&yrs));
        assert_eq!(text_of(&big_paste_doc::<AmDoc>(1_000)).len(), 1_000);
    }

    #[test]
    fn list_and_map_generators_match_across_libraries() {
        let am = list_splice_100::<AmDoc>(5_000);
        let yrs = list_splice_100::<YrsDoc>(5_000);
        assert_eq!(am.list_len(&am.get_list(CONTENT).unwrap()), 5_000);
        assert_eq!(yrs.list_len(&yrs.get_list(CONTENT).unwrap()), 5_000);

        let am = deep_history_doc::<AmDoc>(50);
        let yrs = deep_history_doc::<YrsDoc>(50);
        assert_eq!(am.map_len(&am.root_map()), 2);
        assert_eq!(yrs.map_len(&yrs.root_map()), 2);

        let am = string_map_doc::<AmDoc>(100, |i| i.to_string(), |i| i.to_string());
        let yrs = string_map_doc::<YrsDoc>(100, |i| i.to_string(), |i| i.to_string());
        assert_eq!(am.map_visit_values(&am.root_map()), 100);
        assert_eq!(yrs.map_visit_values(&yrs.root_map()), 100);
    }
}
