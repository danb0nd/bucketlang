//! Editing a bucket in place, and the primitives that make an edit safe.
//!
//! This is language-side because every operation here needs the grammar: the
//! splice cuts at a span the parser recorded, and the oracles rely on the
//! registry's notion of bucket identity. What is *not* here is policy — how much
//! context to retrieve, when to retry, how to render any of it. That belongs to
//! whatever is driving the edit.

use crate::ast::BucketKind;
use crate::error::{Error, Result};
use crate::eval::eval_bucket;
use crate::registry::Registry;
use std::collections::BTreeMap;
use std::io::Write;

/// Address -> content hash for user buckets. Test ids are synthesized and may
/// renumber; cores never change. Neither belongs in an atomicity check.
pub fn user_hashes(reg: &Registry) -> BTreeMap<String, String> {
    reg.buckets
        .iter()
        .filter(|(_, b)| b.kind == BucketKind::User)
        .map(|(a, b)| (a.clone(), b.content_hash.clone()))
        .collect()
}

/// The loop's atomicity rule: an edit replaces exactly one bucket and re-indexes
/// only that node. Anything else means the splice landed somewhere it should not
/// have — this is the gate that catches a body whose braces close early and
/// define, delete, or rewrite neighbouring buckets.
pub fn only_target_changed(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
    target: &str,
) -> std::result::Result<(), String> {
    let mut collateral: Vec<String> = Vec::new();
    for (addr, hash) in after {
        if addr == target {
            continue;
        }
        match before.get(addr) {
            Some(old) if old == hash => {}
            Some(_) => collateral.push(format!("{addr} (body changed)")),
            None => collateral.push(format!("{addr} (appeared)")),
        }
    }
    for addr in before.keys() {
        if !after.contains_key(addr) {
            collateral.push(format!("{addr} (disappeared)"));
        }
    }
    if collateral.is_empty() {
        return Ok(());
    }
    collateral.sort();
    Err(format!(
        "edit was not atomic: only {target} should have changed, but it also touched {}",
        collateral.join(", ")
    ))
}

/// Replace the body of exactly one bucket, located by its recorded span.
///
/// `reg` must be the registry compiled from `source` — the span is a byte offset
/// into that exact text. Resolving through the registry is what makes this precise:
/// the target is an address, so a label mentioned in a `desc` string, a call site
/// appearing before the definition, or a module-qualified name that merely contains
/// the key can no longer be mistaken for the definition.
///
/// Refuses buckets with no span: cores, synthesized `@test` buckets, and anything
/// linked in from an import (whose span belongs to a different file).
pub fn splice_bucket_body(
    source: &str,
    reg: &Registry,
    bucket_key: &str,
    new_body: &str,
) -> Result<String> {
    let key = bucket_key.trim();
    let addr = reg
        .resolve_target(key)
        .ok_or_else(|| Error::msg(format!("unknown bucket {key}")))?;
    let b = reg
        .get(&addr)
        .ok_or_else(|| Error::msg(format!("unknown bucket {key}")))?;

    match b.kind {
        BucketKind::User => {}
        BucketKind::Test => {
            return Err(Error::msg(format!(
                "{key} resolves to a @test bucket ({addr}); edit the @test annotation on its subject instead"
            )));
        }
        BucketKind::Core => {
            return Err(Error::msg(format!(
                "{key} resolves to core {addr}, which has no editable body"
            )));
        }
    }

    let span = b.body_span.ok_or_else(|| {
        Error::msg(format!(
            "bucket {key} ({addr}) is not defined in this file — it is linked in from an import; edit it in its own module"
        ))
    })?;
    if span.start > span.end || span.end > source.len() {
        return Err(Error::msg(format!(
            "span for bucket {key} does not fit this source; recompile before editing"
        )));
    }
    if !source.is_char_boundary(span.start) || !source.is_char_boundary(span.end) {
        return Err(Error::msg(format!(
            "span for bucket {key} is not on a character boundary; recompile before editing"
        )));
    }

    let body = new_body.trim();
    let mut out = String::with_capacity(source.len() + body.len());
    out.push_str(&source[..span.start]);
    out.push('\n');
    for line in body.lines() {
        out.push_str("  ");
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.push_str(&source[span.end..]);
    Ok(out)
}

pub fn run_subject_tests(
    reg: &Registry,
    subject: &str,
    out: &mut dyn Write,
) -> Result<(usize, usize)> {
    let addr = reg.resolve_target(subject).unwrap_or_else(|| subject.to_string());
    let label = reg.get(&addr).and_then(|b| b.label.clone());
    let mut run = 0usize;
    let mut passed = 0usize;
    let mut sink = std::io::sink();
    for tid in &reg.test_ids {
        let b = reg.get(tid).unwrap();
        let related = b.subject.as_ref().is_some_and(|s| {
            s == &addr || label.as_ref().is_some_and(|l| l == s) || s == subject
        });
        if !related && !reg.test_ids.is_empty() {
            // If none are subject-linked we'll fall back below
        }
        if related {
            run += 1;
            let result = eval_bucket(reg, tid, &[], &mut sink);
            if b.expect_error {
                if result.is_err() {
                    passed += 1;
                } else {
                    return Err(Error::msg(format!(
                        "test {tid} expected error but succeeded"
                    )));
                }
            } else {
                result?;
                passed += 1;
            }
        }
    }
    if run == 0 {
        // fall back: run all tests
        for tid in &reg.test_ids {
            let b = reg.get(tid).unwrap();
            run += 1;
            let result = eval_bucket(reg, tid, &[], &mut sink);
            if b.expect_error {
                if result.is_err() {
                    passed += 1;
                } else {
                    return Err(Error::msg(format!(
                        "test {tid} expected error but succeeded"
                    )));
                }
            } else {
                result?;
                passed += 1;
            }
        }
    }
    let _ = out;
    Ok((run, passed))
}