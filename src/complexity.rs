use crate::ast::{Complexity, Expr, Stmt};

pub const MAX_NODES: usize = 32;
pub const MAX_DEPTH: usize = 10;
pub const MAX_CALLS: usize = 8;

pub fn measure(expr: &Expr) -> Complexity {
    fn walk(e: &Expr, depth: usize) -> Complexity {
        match e {
            Expr::Num(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Var(_) => Complexity {
                nodes: 1,
                depth,
                calls: 0,
            },
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
