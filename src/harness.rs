use crate::ast::BucketKind;
use crate::error::{Error, Result};
use crate::eval::eval_bucket;
use crate::graph::build_graph;
use crate::registry::Registry;
use crate::render::render_labelled;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

#[derive(Debug, Serialize)]
pub struct ContextPack {
    pub file_hint: String,
    pub target: ContextBucket,
    pub callees: Vec<ContextBucket>,
    pub callers: Vec<ContextRef>,
    pub tests: Vec<ContextTest>,
    pub cores_used: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ContextBucket {
    pub address: String,
    pub label: Option<String>,
    pub desc: String,
    pub contract: String,
    pub body_labelled: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct ContextRef {
    pub address: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ContextTest {
    pub address: String,
    pub desc: String,
    pub subject: Option<String>,
    pub body_labelled: String,
}

#[derive(Debug, Serialize)]
pub struct EditResult {
    pub ok: bool,
    pub bucket: String,
    pub tests_run: usize,
    pub tests_passed: usize,
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prints: Option<Vec<String>>,
    /// Present on successful dry-run (no `--write`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn contract_str(b: &crate::ast::Bucket) -> String {
    let params: Vec<String> = b
        .contract
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty.name()))
        .collect();
    format!("({}) -> {}", params.join(", "), b.contract.ret.name())
}

fn bucket_pack(reg: &Registry, addr: &str) -> Option<ContextBucket> {
    let b = reg.get(addr)?;
    Some(ContextBucket {
        address: b.address.clone(),
        label: b.label.clone(),
        desc: b.desc.clone(),
        contract: contract_str(b),
        body_labelled: render_labelled(&b.body, reg),
        kind: match b.kind {
            BucketKind::User => "user".into(),
            BucketKind::Test => "test".into(),
            BucketKind::Core => "core".into(),
        },
    })
}

pub fn build_context(reg: &Registry, target: &str, depth: usize) -> Result<ContextPack> {
    let addr = reg
        .resolve_target(target)
        .ok_or_else(|| Error::msg(format!("unknown bucket {target}")))?;
    let g = build_graph(reg);
    let node = g
        .get(&addr)
        .ok_or_else(|| Error::msg(format!("no graph node for {addr}")))?;

    let mut callees = Vec::new();
    let mut cores_used = BTreeSet::new();
    let mut frontier: Vec<(String, usize)> = node.out.iter().cloned().map(|a| (a, 1)).collect();
    let mut seen = BTreeSet::from([addr.clone()]);

    while let Some((id, d)) = frontier.pop() {
        if d > depth || !seen.insert(id.clone()) {
            continue;
        }
        if let Some(b) = reg.get(&id) {
            match b.kind {
                BucketKind::Core => {
                    cores_used.insert(id);
                }
                BucketKind::User => {
                    callees.push(bucket_pack(reg, &id).unwrap());
                    if d < depth {
                        if let Some(n) = g.get(&id) {
                            for o in &n.out {
                                frontier.push((o.clone(), d + 1));
                            }
                        }
                    }
                }
                BucketKind::Test => {}
            }
        }
    }
    callees.sort_by(|a, b| a.address.cmp(&b.address));

    let mut callers = Vec::new();
    for c in &node.inn {
        if let Some(b) = reg.get(c) {
            if b.kind == BucketKind::User {
                callers.push(ContextRef {
                    address: b.address.clone(),
                    label: b.label.clone(),
                });
            }
        }
    }

    let subject_label = reg.get(&addr).and_then(|b| b.label.clone());
    let mut tests = Vec::new();
    for tid in &reg.test_ids {
        if let Some(b) = reg.get(tid) {
            let related = b.subject.as_ref().is_some_and(|s| {
                s == &addr || subject_label.as_ref().is_some_and(|l| l == s)
            });
            if related {
                tests.push(ContextTest {
                    address: b.address.clone(),
                    desc: b.desc.clone(),
                    subject: b.subject.clone(),
                    body_labelled: render_labelled(&b.body, reg),
                });
            }
        }
    }

    Ok(ContextPack {
        file_hint: String::new(),
        target: bucket_pack(reg, &addr).unwrap(),
        callees,
        callers,
        tests,
        cores_used: cores_used.into_iter().collect(),
    })
}

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
