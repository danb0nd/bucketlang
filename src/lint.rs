use crate::ast::{Bucket, BucketKind, Expr, Stmt};
use crate::registry::Registry;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct Warning {
    pub message: String,
}

/// Lint user buckets for unused params / locals. Names starting with `_` are ignored.
pub fn unused_warnings(reg: &Registry) -> Vec<Warning> {
    let mut out = Vec::new();
    for b in reg.buckets.values() {
        if b.kind != BucketKind::User {
            continue;
        }
        lint_bucket(b, &mut out);
    }
    out
}

fn lint_bucket(b: &Bucket, out: &mut Vec<Warning>) {
    let label = b.label.as_deref().unwrap_or(b.address.as_str());
    let body_vars = all_vars(&b.body);
    for p in &b.contract.params {
        if p.name.starts_with('_') {
            continue;
        }
        if !body_vars.contains(&p.name) {
            out.push(Warning {
                message: format!(
                    "unused parameter '{}' in {} ({})",
                    p.name, label, b.address
                ),
            });
        }
    }
    lint_unused_locals(&b.body, label, &b.address, out);
}

fn lint_unused_locals(expr: &Expr, label: &str, addr: &str, out: &mut Vec<Warning>) {
    match expr {
        Expr::Block { stmts, result } => {
            for (i, stmt) in stmts.iter().enumerate() {
                match stmt {
                    Stmt::Bind { name, value } => {
                        lint_unused_locals(value, label, addr, out);
                        if name.starts_with('_') {
                            continue;
                        }
                        let mut later = BTreeSet::new();
                        for s in &stmts[i + 1..] {
                            later.extend(vars_in_stmt(s));
                        }
                        later.extend(all_vars(result));
                        if !later.contains(name) {
                            out.push(Warning {
                                message: format!(
                                    "unused local '{name}' in {label} ({addr})"
                                ),
                            });
                        }
                    }
                    Stmt::Run(e) => lint_unused_locals(e, label, addr, out),
                }
            }
            lint_unused_locals(result, label, addr, out);
        }
        Expr::Call { args, .. } => {
            for a in args {
                lint_unused_locals(a, label, addr, out);
            }
        }
        Expr::Num(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Var(_) => {}
    }
}

fn vars_in_stmt(stmt: &Stmt) -> BTreeSet<String> {
    match stmt {
        Stmt::Bind { value, .. } => all_vars(value),
        Stmt::Run(e) => all_vars(e),
    }
}

fn all_vars(expr: &Expr) -> BTreeSet<String> {
    let mut set = BTreeSet::new();
    collect_vars(expr, &mut set);
    set
}

fn collect_vars(expr: &Expr, set: &mut BTreeSet<String>) {
    match expr {
        Expr::Var(name) => {
            set.insert(name.clone());
        }
        Expr::Num(_) | Expr::Bool(_) | Expr::Str(_) => {}
        Expr::Call { args, .. } => {
            for a in args {
                collect_vars(a, set);
            }
        }
        Expr::Block { stmts, result } => {
            for s in stmts {
                match s {
                    Stmt::Bind { value, .. } => collect_vars(value, set),
                    Stmt::Run(e) => collect_vars(e, set),
                }
            }
            collect_vars(result, set);
        }
    }
}
