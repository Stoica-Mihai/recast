//! Single-pass rename maps.
//!
//! A rename map applies N name changes in **one** traversal, so a set of
//! dependent renames can't collapse into each other. Run as separate
//! invocations, `Foo -> Bar` followed by `Bar -> Baz` turns the original
//! `Foo` and the original `Bar` into the same `Baz`, and every guard
//! passes on both runs because each is convergent on its own.
//!
//! Keys are matched as whole words, literally — this is a map over
//! identifiers, not a regex pipeline. That is also what makes the
//! stability analysis below meaningful: "does this replacement contain
//! one of my keys" is a decidable question for literal words and not for
//! arbitrary patterns.

use std::collections::{HashMap, HashSet};

use regex::Regex;

use crate::error::{Error, Result};
use crate::rewrite::RewriteOutcome;

/// Ceiling on a replacement's length while probing for stability.
const STABILITY_LEN_CAP: usize = 4096;

/// A validated set of whole-word renames applied in a single pass.
#[derive(Debug, Clone)]
pub struct RenameMap {
    pairs: Vec<(String, String)>,
    lookup: HashMap<String, String>,
    regex: Regex,
    rerunnable: bool,
}

impl RenameMap {
    /// Validate `pairs` and compile them into one alternation.
    ///
    /// Rejects an empty map, an empty key, a duplicate key, and a map
    /// that does not settle — see [`RenameMap::rerunnable`] for what
    /// "settle" means and which shapes are deliberately allowed.
    pub fn new(pairs: Vec<(String, String)>) -> Result<Self> {
        if pairs.is_empty() {
            return Err(Error::InvalidRenameMap { reason: "no renames given".to_owned() });
        }
        let mut lookup: HashMap<String, String> = HashMap::with_capacity(pairs.len());
        for (key, value) in &pairs {
            if key.is_empty() {
                return Err(Error::InvalidRenameMap {
                    reason: "empty name on the left".to_owned(),
                });
            }
            if lookup.insert(key.clone(), value.clone()).is_some() {
                return Err(Error::InvalidRenameMap { reason: format!("`{key}` renamed twice") });
            }
        }

        // The regex crate's alternation is leftmost-first, so a shorter
        // key listed before a longer one that shares its prefix would
        // shadow it. Ordering by descending length removes the
        // dependence on the caller's ordering.
        let mut keys: Vec<&str> = lookup.keys().map(String::as_str).collect();
        keys.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        let alternation = keys.iter().map(|k| regex::escape(k)).collect::<Vec<_>>().join("|");
        let regex = Regex::new(&format!(r"\b{{start-half}}(?:{alternation})\b{{end-half}}"))?;

        let mut map = Self { pairs, lookup, regex, rerunnable: false };
        map.rerunnable = map.probe_stability()?;
        Ok(map)
    }

    /// Rewrite every whole-word occurrence of a key in `text`, once.
    pub fn rewrite(&self, text: &str) -> RewriteOutcome {
        let mut matches = 0usize;
        let after = self
            .regex
            .replace_all(text, |caps: &regex::Captures<'_>| {
                matches += 1;
                let hit = &caps[0];
                // The alternation only matches keys, so the lookup
                // cannot miss; leaving the text alone is the harmless
                // direction if it ever did.
                self.lookup.get(hit).cloned().unwrap_or_else(|| hit.to_owned())
            })
            .into_owned();
        RewriteOutcome { after, matches }
    }

    /// True when re-running the map is a no-op, i.e. no replacement
    /// contains one of the keys. A swap (`Foo -> Bar, Bar -> Foo`) and a
    /// chain (`Foo -> Bar, Bar -> Baz`) are both correct exactly once
    /// and report `false`: they are accepted, but running them twice
    /// keeps rewriting.
    pub fn rerunnable(&self) -> bool {
        self.rerunnable
    }

    pub fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }

    /// Re-apply the map to each of its own replacements until the text
    /// stops changing or repeats.
    ///
    /// - Settles immediately → no replacement contains a key → the map
    ///   is re-runnable.
    /// - Settles after a few rounds, or returns to a string already
    ///   seen → a chain or a cycle. Bounded, so correct exactly once.
    /// - Never settles → the map feeds itself and grows. Refused.
    fn probe_stability(&self) -> Result<bool> {
        let rounds = (self.pairs.len() * 2 + 2).max(64);
        let mut rerunnable = true;
        for (key, value) in &self.pairs {
            let mut seen: HashSet<String> = HashSet::new();
            seen.insert(value.clone());
            let mut current = value.clone();
            let mut settled = false;
            for _ in 0..rounds {
                let next = self.rewrite(&current).after;
                if next == current {
                    settled = true;
                    break;
                }
                rerunnable = false;
                if next.len() > STABILITY_LEN_CAP {
                    break;
                }
                if !seen.insert(next.clone()) {
                    settled = true;
                    break;
                }
                current = next;
            }
            if !settled {
                return Err(Error::RenameMapDiverges {
                    key: key.clone(),
                    value: value.clone(),
                    rounds,
                });
            }
        }
        Ok(rerunnable)
    }
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;
