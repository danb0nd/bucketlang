use crate::ast::{Bucket, BucketKind, Expr, ExprKind, Stmt};
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
    match &expr.kind {
        ExprKind::Block { stmts, result } => {
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
        ExprKind::Call { args, .. } => {
            for a in args {
                lint_unused_locals(a, label, addr, out);
            }
        }
        ExprKind::List(elems) => {
            for e in elems {
                lint_unused_locals(e, label, addr, out);
            }
        }
        ExprKind::Record(fields) => {
            for (_, v) in fields {
                lint_unused_locals(v, label, addr, out);
            }
        }
        ExprKind::Field { base, .. } => lint_unused_locals(base, label, addr, out),
        ExprKind::Variant { payload, .. } => {
            if let Some(p) = payload {
                lint_unused_locals(p, label, addr, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            lint_unused_locals(scrutinee, label, addr, out);
            for a in arms {
                lint_unused_locals(&a.body, label, addr, out);
            }
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            lint_unused_locals(cond, label, addr, out);
            lint_unused_locals(then_branch, label, addr, out);
            lint_unused_locals(else_branch, label, addr, out);
        }
        ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Var(_) => {}
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
    match &expr.kind {
        ExprKind::Var(name) => {
            set.insert(name.clone());
        }
        ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) => {}
        ExprKind::List(elems) => {
            for e in elems {
                collect_vars(e, set);
            }
        }
        ExprKind::Record(fields) => {
            for (_, v) in fields {
                collect_vars(v, set);
            }
        }
        ExprKind::Field { base, .. } => collect_vars(base, set),
        ExprKind::Variant { payload, .. } => {
            if let Some(p) = payload {
                collect_vars(p, set);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_vars(scrutinee, set);
            for a in arms {
                collect_vars(&a.body, set);
            }
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_vars(cond, set);
            collect_vars(then_branch, set);
            collect_vars(else_branch, set);
        }
        ExprKind::Call { args, .. } => {
            for a in args {
                collect_vars(a, set);
            }
        }
        ExprKind::Block { stmts, result } => {
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
