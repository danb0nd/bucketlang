use crate::ast::{Complexity, Expr, Stmt};

pub const MAX_NODES: usize = 32;
pub const MAX_DEPTH: usize = 10;
pub const MAX_CALLS: usize = 12;

pub fn measure(expr: &Expr) -> Complexity {
    fn walk(e: &Expr, depth: usize) -> Complexity {
        match e {
            Expr::Num(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Var(_) => Complexity {
                nodes: 1,
                depth,
                calls: 0,
            },
            Expr::List(elems) => {
                let mut nodes = 1;
                let mut calls = 0;
                let mut max_d = depth;
                for a in elems {
                    let c = walk(a, depth + 1);
                    nodes += c.nodes;
                    calls += c.calls;
                    max_d = max_d.max(c.depth);
                }
                Complexity {
                    nodes,
                    depth: max_d,
                    calls,
                }
            }
            Expr::Record(fields) => {
                let mut nodes = 1;
                let mut calls = 0;
                let mut max_d = depth;
                for (_, v) in fields {
                    let c = walk(v, depth + 1);
                    nodes += c.nodes;
                    calls += c.calls;
                    max_d = max_d.max(c.depth);
                }
                Complexity {
                    nodes,
                    depth: max_d,
                    calls,
                }
            }
            Expr::Field { base, .. } => {
                let c = walk(base, depth + 1);
                Complexity {
                    nodes: c.nodes + 1,
                    depth: c.depth,
                    calls: c.calls,
                }
            }
            Expr::Variant { payload, .. } => match payload {
                None => Complexity {
                    nodes: 1,
                    depth,
                    calls: 0,
                },
                Some(p) => {
                    let c = walk(p, depth + 1);
                    Complexity {
                        nodes: c.nodes + 1,
                        depth: c.depth,
                        calls: c.calls,
                    }
                }
            },
            Expr::Match { scrutinee, arms } => {
                let mut nodes = 1;
                let mut calls = 0;
                let mut max_d = depth;
                let c = walk(scrutinee, depth + 1);
                nodes += c.nodes;
                calls += c.calls;
                max_d = max_d.max(c.depth);
                for a in arms {
                    let c = walk(&a.body, depth + 1);
                    nodes += c.nodes;
                    calls += c.calls;
                    max_d = max_d.max(c.depth);
                }
                Complexity {
                    nodes,
                    depth: max_d,
                    calls,
                }
            }
            Expr::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let mut nodes = 1;
                let mut calls = 0;
                let mut max_d = depth;
                for part in [cond.as_ref(), then_branch.as_ref(), else_branch.as_ref()] {
                    let c = walk(part, depth + 1);
                    nodes += c.nodes;
                    calls += c.calls;
                    max_d = max_d.max(c.depth);
                }
                Complexity {
                    nodes,
                    depth: max_d,
                    calls,
                }
            }
            Expr::Call { args, .. } => {
                let mut nodes = 1;
                let mut calls = 1;
                let mut max_d = depth;
                for a in args {
                    let c = walk(a, depth + 1);
                    nodes += c.nodes;
                    calls += c.calls;
                    max_d = max_d.max(c.depth);
                }
                Complexity {
                    nodes,
                    depth: max_d,
                    calls,
                }
            }
            Expr::Block { stmts, result } => {
                let mut nodes = 1;
                let mut calls = 0;
                let mut max_d = depth;
                for s in stmts {
                    let e = match s {
                        Stmt::Bind { value, .. } => value,
                        Stmt::Run(e) => e,
                    };
                    let c = walk(e, depth + 1);
                    nodes += c.nodes;
                    calls += c.calls;
                    max_d = max_d.max(c.depth);
                }
                let c = walk(result, depth + 1);
                nodes += c.nodes;
                calls += c.calls;
                max_d = max_d.max(c.depth);
                Complexity {
                    nodes,
                    depth: max_d,
                    calls,
                }
            }
        }
    }
    walk(expr, 1)
}

pub fn within_budget(c: &Complexity) -> Result<(), String> {
    if c.nodes > MAX_NODES {
        return Err(format!(
            "complexity: nodes {} exceeds max {MAX_NODES}",
            c.nodes
        ));
    }
    if c.depth > MAX_DEPTH {
        return Err(format!(
            "complexity: depth {} exceeds max {MAX_DEPTH}",
            c.depth
        ));
    }
    if c.calls > MAX_CALLS {
        return Err(format!(
            "complexity: calls {} exceeds max {MAX_CALLS}",
            c.calls
        ));
    }
    Ok(())
}
