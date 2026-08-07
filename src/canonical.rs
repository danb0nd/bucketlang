use crate::ast::{Expr, Stmt};

pub fn canonical_repr(expr: &Expr) -> String {
    match expr {
        Expr::Num(n) => format!("N({n})"),
        Expr::Bool(b) => format!("B({b})"),
        Expr::Str(s) => format!("S({s:?})"),
        Expr::Var(p) => format!("V({p})"),
        Expr::Call { target, args } => {
            let inner: Vec<String> = args.iter().map(canonical_repr).collect();
            format!("C({target};{})", inner.join(","))
        }
        Expr::Block { stmts, result } => {
            let ss: Vec<String> = stmts
                .iter()
                .map(|s| match s {
                    Stmt::Bind { name, value } => format!("B({name}={})", canonical_repr(value)),
                    Stmt::Run(e) => format!("R({})", canonical_repr(e)),
                })
                .collect();
            format!("BLK[{};{}]", ss.join(";"), canonical_repr(result))
        }
    }
}

pub fn content_hash(expr: &Expr) -> String {
    use sha2::{Digest, Sha256};
    let s = canonical_repr(expr);
    let digest = Sha256::digest(s.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect::<String>()[..16].to_string()
}
