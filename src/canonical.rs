use crate::ast::{Expr, Stmt};

pub fn canonical_repr(expr: &Expr) -> String {
    match expr {
        Expr::Num(n) => format!("N({n})"),
        Expr::Bool(b) => format!("B({b})"),
        Expr::Str(s) => format!("S({s:?})"),
        Expr::Var(p) => format!("V({p})"),
        Expr::List(elems) => {
            let inner: Vec<String> = elems.iter().map(canonical_repr).collect();
            format!("L[{}]", inner.join(","))
        }
        Expr::Record(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}={}", canonical_repr(v)))
                .collect();
            format!("R{{{}}}", inner.join(","))
        }
        Expr::Field { base, field } => format!("F({}.{})", canonical_repr(base), field),
        Expr::Variant { tag, payload } => match payload {
            None => format!("V({tag})"),
            Some(p) => format!("V({tag};{})", canonical_repr(p)),
        },
        Expr::Match { scrutinee, arms } => {
            let as_: Vec<String> = arms
                .iter()
                .map(|a| {
                    let b = a.binder.as_deref().unwrap_or("_");
                    format!("{}({}):{}", a.tag, b, canonical_repr(&a.body))
                })
                .collect();
            format!("M({};{})", canonical_repr(scrutinee), as_.join("|"))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => format!(
            "IF({},{},{})",
            canonical_repr(cond),
            canonical_repr(then_branch),
            canonical_repr(else_branch)
        ),
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
