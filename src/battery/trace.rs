//! Editing traces: loading and replaying them into any [`Crdt`].
//!
//! Two formats are supported:
//! - The Eg-walker / josephg editing-traces format: `{ endContent, numAgents,
//!   txns: [{ parents, numChildren, agent, patches: [[pos, del, ins]] }] }`.
//!   Transactions form a DAG; [`replay`] handles any number of agents.
//! - The Automerge battery's flat `edits.json`: `[[pos, del, ...chars]]`.
//!
//! Positions are Unicode code points.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

use crate::api::{Crdt, Txn};

/// Root key of the text object in replayed documents. Matches the battery.
pub const TEXT: &str = "text";

const ROOT_BRANCH: usize = usize::MAX;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    pub end_content: String,
    pub num_agents: usize,
    pub txns: Vec<Transaction>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub parents: Vec<usize>,
    pub num_children: usize,
    pub agent: usize,
    pub patches: Vec<Patch>,
}

#[derive(Debug, Deserialize)]
pub struct Patch(pub usize, pub usize, pub String);

impl Trace {
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
        let trace: Self = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("parsing {}", path.display()))?;
        trace
            .validate()
            .with_context(|| format!("validating {}", path.display()))?;
        Ok(trace)
    }

    pub fn patch_count(&self) -> usize {
        self.txns.iter().map(|t| t.patches.len()).sum()
    }

    /// Checks the DAG is well formed and returns the number of root transactions.
    ///
    /// Also checks that each agent's transactions form a single chain: every
    /// transaction descends from all earlier transactions by the same agent.
    /// The replayer gives each agent one fixed replica ID, and libraries that
    /// number edits per replica (Yrs) corrupt the document when two concurrent
    /// branches share one.
    fn validate(&self) -> Result<usize> {
        if self.txns.is_empty() {
            bail!("trace contains no transactions");
        }
        let mut actual_children = vec![0usize; self.txns.len()];
        let mut roots = 0;
        let mut finals = 0;
        for (index, txn) in self.txns.iter().enumerate() {
            if txn.agent >= self.num_agents {
                bail!(
                    "transaction {index} uses agent {}, but numAgents is {}",
                    txn.agent,
                    self.num_agents
                );
            }
            roots += txn.parents.is_empty() as usize;
            finals += (txn.num_children == 0) as usize;
            for &parent in &txn.parents {
                if parent >= index {
                    bail!("transaction {index} has non-ancestor parent {parent}");
                }
                actual_children[parent] += 1;
            }
        }
        if roots == 0 {
            bail!("trace has no root transactions");
        }
        if finals != 1 {
            bail!("trace must have one final transaction, found {finals}");
        }
        for (index, (txn, actual)) in self.txns.iter().zip(actual_children).enumerate() {
            if txn.num_children != actual {
                bail!(
                    "transaction {index} declares {} children but has {actual}",
                    txn.num_children
                );
            }
        }
        self.check_agent_chains()?;
        Ok(roots)
    }

    /// Tracks, per transaction, how many of each agent's transactions are its
    /// ancestors (inclusive). By induction every agent's ancestors form a
    /// prefix of its chain, so a pointwise max over parents is exact. A clock
    /// is dropped once all of its transaction's children have read it.
    fn check_agent_chains(&self) -> Result<()> {
        let mut issued = vec![0u32; self.num_agents];
        let mut clocks: HashMap<usize, (Vec<u32>, usize)> = HashMap::new();
        for (index, txn) in self.txns.iter().enumerate() {
            let mut clock = vec![0u32; self.num_agents];
            for &parent in &txn.parents {
                let (parent_clock, remaining) = clocks
                    .get_mut(&parent)
                    .expect("parents precede children and are kept until last use");
                for (mine, theirs) in clock.iter_mut().zip(parent_clock.iter()) {
                    *mine = (*mine).max(*theirs);
                }
                *remaining -= 1;
                if *remaining == 0 {
                    clocks.remove(&parent);
                }
            }
            if clock[txn.agent] != issued[txn.agent] {
                bail!(
                    "transaction {index} by agent {} is concurrent with {} of that agent's \
                     earlier transactions; one replica ID per agent would be unsafe",
                    txn.agent,
                    issued[txn.agent] - clock[txn.agent]
                );
            }
            issued[txn.agent] += 1;
            clock[txn.agent] += 1;
            if txn.num_children > 0 {
                clocks.insert(index, (clock, txn.num_children));
            }
        }
        Ok(())
    }
}

/// Replay a trace, reproducing its concurrency.
///
/// Every transaction runs on a copy of its first parent's document (forked
/// with the transaction's agent), after merging in its other parents. A
/// parent's document is reused, not copied, by its last child.
///
/// The text object lives at [`TEXT`]. `progress` is called with the number of
/// patches applied so far.
pub fn replay<D: Crdt>(trace: &Trace, mut progress: impl FnMut(usize)) -> D {
    let roots = trace.validate().expect("trace was validated on load");

    let root = D::trace_root(TEXT);
    let mut branches: HashMap<usize, (D, usize)> = HashMap::new();
    branches.insert(ROOT_BRANCH, (root, roots));

    let mut applied = 0;
    let mut result = None;
    for (index, txn) in trace.txns.iter().enumerate() {
        let (first, others) = txn
            .parents
            .split_first()
            .map_or((ROOT_BRANCH, &[][..]), |(first, rest)| (*first, rest));

        let mut doc = take_branch(&mut branches, first, Some(txn.agent as u64));
        for &parent in others {
            let mut other = take_branch(&mut branches, parent, None);
            doc.merge(&mut other);
        }

        // Text handles belong to one document, so look it up on each copy.
        let text = doc.get_text(TEXT).expect("replayed docs have a text");
        {
            let mut tx = doc.transact();
            let mut len = tx.text_len(&text);
            for Patch(position, delete, insert) in &txn.patches {
                // Some traces have positions past the end of the text; clamp
                // them, as the Eg-walker converter does.
                let start = (*position).min(len);
                let end = position.saturating_add(*delete).min(len);
                tx.splice_text(&text, start, end - start, insert);
                len = len - (end - start) + insert.chars().count();
            }
        }
        applied += txn.patches.len();
        progress(applied);

        if txn.num_children == 0 {
            result = Some(doc);
        } else {
            branches.insert(index, (doc, txn.num_children));
        }
    }
    result.expect("validated traces have a final transaction")
}

fn take_branch<D: Crdt>(
    branches: &mut HashMap<usize, (D, usize)>,
    index: usize,
    agent: Option<u64>,
) -> D {
    let (parent, remaining_uses) = branches
        .get_mut(&index)
        .unwrap_or_else(|| panic!("transaction refers to unavailable parent {index}"));
    if *remaining_uses == 1 {
        // The last user takes the document itself.
        let (mut doc, _) = branches.remove(&index).expect("branch was just found");
        if let Some(agent) = agent {
            doc.set_agent(agent);
        }
        doc
    } else {
        *remaining_uses -= 1;
        match agent {
            Some(agent) => parent.fork_as(agent),
            None => parent.fork(),
        }
    }
}

/// One edit from the flat `edits.json` format.
#[derive(Clone, Debug)]
pub struct Edit {
    pub pos: usize,
    pub delete: usize,
    pub insert: String,
}

/// Load `[[pos, del, ...chars]]`.
pub fn load_edits(path: &Path) -> Result<Vec<Edit>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let raw: Vec<Vec<serde_json::Value>> = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing {}", path.display()))?;
    raw.into_iter()
        .enumerate()
        .map(|(i, edit)| {
            let number = |v: Option<&serde_json::Value>| {
                v.and_then(serde_json::Value::as_u64)
                    .with_context(|| format!("edit {i}: expected a number"))
            };
            let insert = edit[2..]
                .iter()
                .map(|v| v.as_str().with_context(|| format!("edit {i}: expected a string")))
                .collect::<Result<String>>()?;
            Ok(Edit {
                pos: number(edit.first())? as usize,
                delete: number(edit.get(1))? as usize,
                insert,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{AmDoc, YrsDoc};

    /// Two agents edit concurrently from a shared base, then a third
    /// transaction merges both.
    fn diamond() -> Trace {
        serde_json::from_str(
            r#"{
              "endContent": "",
              "numAgents": 3,
              "txns": [
                {"parents": [], "numChildren": 2, "agent": 0, "patches": [[0, 0, "hello"]]},
                {"parents": [0], "numChildren": 1, "agent": 1, "patches": [[5, 0, " world"]]},
                {"parents": [0], "numChildren": 1, "agent": 2, "patches": [[0, 1, "J"]]},
                {"parents": [1, 2], "numChildren": 0, "agent": 0, "patches": [[0, 0, "> "]]}
              ]
            }"#,
        )
        .unwrap()
    }

    fn replayed_text<D: Crdt>(trace: &Trace) -> String {
        let doc: D = replay(trace, |_| {});
        doc.text(&doc.get_text(TEXT).unwrap())
    }

    #[test]
    fn replays_concurrent_branches() {
        let trace = diamond();
        assert_eq!(replayed_text::<AmDoc>(&trace), "> Jello world");
        assert_eq!(replayed_text::<YrsDoc>(&trace), "> Jello world");
    }

    #[test]
    fn rejects_concurrent_transactions_by_one_agent() {
        // Transactions 1 and 2 are both by agent 1 but neither descends from
        // the other.
        let trace: Trace = serde_json::from_str(
            r#"{
              "endContent": "",
              "numAgents": 2,
              "txns": [
                {"parents": [], "numChildren": 2, "agent": 0, "patches": [[0, 0, "a"]]},
                {"parents": [0], "numChildren": 1, "agent": 1, "patches": [[0, 0, "b"]]},
                {"parents": [0], "numChildren": 1, "agent": 1, "patches": [[0, 0, "c"]]},
                {"parents": [1, 2], "numChildren": 0, "agent": 0, "patches": []}
              ]
            }"#,
        )
        .unwrap();
        let error = trace.validate().unwrap_err().to_string();
        assert!(error.contains("transaction 2 by agent 1"), "{error}");
        assert!(diamond().validate().is_ok());
    }
}
