use crate::ast::{Expr, Stmt};
use crate::registry::Registry;
use crate::value::format_num;

pub fn render_raw(expr: &Expr) -> String {
    match expr {
        Expr::Num(n) => format_num(*n),
        Expr::Bool(b) => b.to_string(),
        Expr::Str(s) => format!("{s:?}"),
        Expr::Var(p) => p.clone(),
        Expr::List(elems) => {
            let inner: Vec<String> = elems.iter().map(render_raw).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::Record(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", render_raw(v)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        Expr::Field { base, field } => format!("{}.{}", render_raw(base), field),
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => format!(
            "if {} then {} else {}",
            render_raw(cond),
            render_raw(then_branch),
            render_raw(else_branch)
        ),
        Expr::Call { target, args } => {
            let inner: Vec<String> = args.iter().map(render_raw).collect();
            format!("{target}({})", inner.join(", "))
        }
        Expr::Block { stmts, result } => {
            let mut parts: Vec<String> = stmts
                .iter()
                .map(|s| match s {
                    Stmt::Bind { name, value } => format!("{name} = {}", render_raw(value)),
                    Stmt::Run(e) => render_raw(e),
                })
                .collect();
            parts.push(render_raw(result));
            format!("{{ {} }}", parts.join("; "))
        }
    }
}

pub fn render_labelled(expr: &Expr, reg: &Registry) -> String {
    match expr {
        Expr::Num(n) => format_num(*n),
        Expr::Bool(b) => b.to_string(),
        Expr::Str(s) => format!("{s:?}"),
        Expr::Var(p) => p.clone(),
        Expr::List(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| render_labelled(e, reg)).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::Record(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", render_labelled(v, reg)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        Expr::Field { base, field } => format!("{}.{}", render_labelled(base, reg), field),
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => format!(
            "if {} then {} else {}",
            render_labelled(cond, reg),
            render_labelled(then_branch, reg),
            render_labelled(else_branch, reg)
        ),
        Expr::Call { target, args } => {
            let name = reg
                .get(target)
                .and_then(|b| b.label.clone())
                .unwrap_or_else(|| target.clone());
            if matches!(
                target.as_str(),
                "#c.add"
                    | "#c.sub"
                    | "#c.mul"
                    | "#c.div"
                    | "#c.eq"
                    | "#c.ne"
                    | "#c.lt"
                    | "#c.gt"
                    | "#c.le"
                    | "#c.ge"
                    | "#c.and"
                    | "#c.or"
            ) && args.len() == 2
            {
                let op = match target.as_str() {
                    "#c.add" => "+",
                    "#c.sub" => "-",
                    "#c.mul" => "*",
                    "#c.div" => "/",
                    "#c.eq" => "==",
                    "#c.ne" => "!=",
                    "#c.lt" => "<",
                    "#c.gt" => ">",
                    "#c.le" => "<=",
                    "#c.ge" => ">=",
                    "#c.and" => "&&",
                    "#c.or" => "||",
                    _ => "?",
                };
                return format!(
                    "({} {} {})",
                    render_labelled(&args[0], reg),
                    op,
                    render_labelled(&args[1], reg)
                );
            }
            if target == "#c.not" && args.len() == 1 {
                return format!("!{}", render_labelled(&args[0], reg));
            }
            let inner: Vec<String> = args.iter().map(|a| render_labelled(a, reg)).collect();
            format!("{name}({})", inner.join(", "))
        }
        Expr::Block { stmts, result } => {
            let mut parts: Vec<String> = stmts
                .iter()
                .map(|s| match s {
                    Stmt::Bind { name, value } => {
                        format!("{name} = {}", render_labelled(value, reg))
                    }
                    Stmt::Run(e) => render_labelled(e, reg),
                })
                .collect();
            parts.push(render_labelled(result, reg));
            format!("{{ {} }}", parts.join("; "))
        }
    }
}
