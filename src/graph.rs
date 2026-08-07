use crate::ast::{BucketKind, Expr, ExprKind};
use crate::registry::Registry;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct GraphNode {
    pub out: BTreeSet<String>,
    pub inn: BTreeSet<String>,
    pub kind: BucketKind,
    pub label: Option<String>,
    pub is_entry: bool,
}

pub type Graph = BTreeMap<String, GraphNode>;

pub fn build_graph(reg: &Registry) -> Graph {
    let mut g: Graph = BTreeMap::new();
    for (id, b) in &reg.buckets {
        g.insert(
            id.clone(),
            GraphNode {
                out: BTreeSet::new(),
                inn: BTreeSet::new(),
                kind: b.kind,
                label: b.label.clone(),
                is_entry: reg.entry.as_deref() == Some(id.as_str()),
            },
        );
    }
    for (id, b) in &reg.buckets {
        if b.kind == BucketKind::Core {
            continue;
        }
        let mut outs = BTreeSet::new();
        collect_calls(&b.body, &mut outs);
        if let Some(node) = g.get_mut(id) {
            node.out = outs.clone();
        }
        for o in outs {
            if let Some(n) = g.get_mut(&o) {
                n.inn.insert(id.clone());
            }
        }
    }
    g
}

fn collect_calls(expr: &Expr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Call { target, args } => {
            out.insert(target.clone());
            for a in args {
                collect_calls(a, out);
            }
        }
        ExprKind::Block { stmts, result } => {
            for s in stmts {
                match s {
                    crate::ast::Stmt::Bind { value, .. } => collect_calls(value, out),
                    crate::ast::Stmt::Run(e) => collect_calls(e, out),
                }
            }
            collect_calls(result, out);
        }
        ExprKind::List(elems) => {
            for e in elems {
                collect_calls(e, out);
            }
        }
        ExprKind::Record(fields) => {
            for (_, v) in fields {
                collect_calls(v, out);
            }
        }
        ExprKind::Field { base, .. } => collect_calls(base, out),
        ExprKind::Variant { payload, .. } => {
            if let Some(p) = payload {
                collect_calls(p, out);
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_calls(scrutinee, out);
            for a in arms {
                collect_calls(&a.body, out);
            }
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_calls(cond, out);
            collect_calls(then_branch, out);
            collect_calls(else_branch, out);
        }
        ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Var(_) => {}
    }
}

pub fn to_dot(graph: &Graph) -> String {
    let mut s = String::from("digraph bucketlang {\n  rankdir=LR;\n");
    for (id, node) in graph {
        let label = match &node.label {
            Some(l) => format!("{id}\\n{l}"),
            None => id.clone(),
        };
        let shape = match node.kind {
            BucketKind::Core => "plaintext",
            BucketKind::Test => "box",
            BucketKind::User => "oval",
        };
        let extra = if node.is_entry { ",peripheries=2" } else { "" };
        s.push_str(&format!(
            "  \"{id}\" [label=\"{label}\",shape={shape}{extra}];\n"
        ));
        for o in &node.out {
            s.push_str(&format!("  \"{id}\" -> \"{o}\";\n"));
        }
    }
    s.push_str("}\n");
    s
}
