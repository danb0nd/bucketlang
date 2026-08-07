use crate::ast::BucketKind;
use crate::error::{Error, Result};
use crate::eval::eval_bucket;
use crate::graph::build_graph;
use crate::registry::Registry;
use crate::render::render_labelled;
use serde::Serialize;
use std::collections::BTreeSet;
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

/// Replace the brace-body of the first definition of `label` (or `#addr`) in source.
pub fn splice_bucket_body(source: &str, bucket_key: &str, new_body: &str) -> Result<String> {
    let key = bucket_key.trim();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // skip line comments
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if starts_with_key(&chars, i, key) {
            // ensure not a longer ident prefix for labels
            if !key.starts_with('#') {
                let after = i + key.chars().count();
                if after < chars.len() {
                    let c = chars[after];
                    if c.is_ascii_alphanumeric() || c == '_' {
                        i += 1;
                        continue;
                    }
                }
            }
            // find opening brace of this bucket
            let mut j = i + key.chars().count();
            while j < chars.len() && chars[j] != '{' {
                if j + 1 < chars.len() && chars[j] == '/' && chars[j + 1] == '/' {
                    j += 2;
                    while j < chars.len() && chars[j] != '\n' {
                        j += 1;
                    }
                    continue;
                }
                // another top-level-looking definition before '{'? keep scanning
                j += 1;
            }
            if j >= chars.len() || chars[j] != '{' {
                return Err(Error::msg(format!(
                    "could not find opening '{{' for bucket {key}"
                )));
            }
            let body_start = j + 1;
            let mut depth = 1usize;
            let mut k = body_start;
            while k < chars.len() {
                let c = chars[k];
                if c == '"' {
                    k += 1;
                    while k < chars.len() {
                        if chars[k] == '\\' {
                            k += 2;
                            continue;
                        }
                        if chars[k] == '"' {
                            k += 1;
                            break;
                        }
                        k += 1;
                    }
                    continue;
                }
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        let before: String = chars[..body_start].iter().collect();
                        let after: String = chars[k..].iter().collect();
                        let body = new_body.trim();
                        let mut out = before;
                        if !body.is_empty() {
                            out.push('\n');
                            for line in body.lines() {
                                out.push_str("  ");
                                out.push_str(line.trim_end());
                                out.push('\n');
                            }
                        } else {
                            out.push('\n');
                        }
                        out.push_str(&after);
                        return Ok(out);
                    }
                }
                k += 1;
            }
            return Err(Error::msg(format!(
                "unclosed '{{' while splicing bucket {key}"
            )));
        }
        i += 1;
    }
    Err(Error::msg(format!(
        "could not find bucket definition for {key} in source"
    )))
}

fn starts_with_key(chars: &[char], i: usize, key: &str) -> bool {
    let key_chars: Vec<char> = key.chars().collect();
    if i + key_chars.len() > chars.len() {
        return false;
    }
    chars[i..i + key_chars.len()] == key_chars[..]
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
            eval_bucket(reg, tid, &[], &mut sink)?;
            passed += 1;
        }
    }
    if run == 0 {
        // fall back: run all tests
        for tid in &reg.test_ids {
            run += 1;
            eval_bucket(reg, tid, &[], &mut sink)?;
            passed += 1;
        }
    }
    let _ = out;
    Ok((run, passed))
}
