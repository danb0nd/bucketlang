use crate::ast::{Expr, ExprKind, Stmt};
use crate::registry::Registry;
use crate::value::format_num;

pub fn render_raw(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Num(n) => format_num(*n),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Str(s) => format!("{s:?}"),
        ExprKind::Var(p) => p.clone(),
        ExprKind::List(elems) => {
            let inner: Vec<String> = elems.iter().map(render_raw).collect();
            format!("[{}]", inner.join(", "))
        }
        ExprKind::Record(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", render_raw(v)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        ExprKind::Field { base, field } => format!("{}.{}", render_raw(base), field),
        ExprKind::Variant { tag, payload } => match payload {
            None => tag.clone(),
            Some(p) => format!("{tag}({})", render_raw(p)),
        },
        ExprKind::Match { scrutinee, arms } => {
            let as_: Vec<String> = arms
                .iter()
                .map(|a| {
                    let pat = match &a.binder {
                        None => a.tag.clone(),
                        Some(b) => format!("{}({})", a.tag, b),
                    };
                    format!("{} => {}", pat, render_raw(&a.body))
                })
                .collect();
            format!("match {} {{ {} }}", render_raw(scrutinee), as_.join(", "))
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => format!(
            "if {} then {} else {}",
            render_raw(cond),
            render_raw(then_branch),
            render_raw(else_branch)
        ),
        ExprKind::Call { target, args } => {
            let inner: Vec<String> = args.iter().map(render_raw).collect();
            format!("{target}({})", inner.join(", "))
        }
        ExprKind::Block { stmts, result } => {
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
    match &expr.kind {
        ExprKind::Num(n) => format_num(*n),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Str(s) => format!("{s:?}"),
        ExprKind::Var(p) => p.clone(),
        ExprKind::List(elems) => {
            let inner: Vec<String> = elems.iter().map(|e| render_labelled(e, reg)).collect();
            format!("[{}]", inner.join(", "))
        }
        ExprKind::Record(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", render_labelled(v, reg)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        ExprKind::Field { base, field } => format!("{}.{}", render_labelled(base, reg), field),
        ExprKind::Variant { tag, payload } => match payload {
            None => tag.clone(),
            Some(p) => format!("{tag}({})", render_labelled(p, reg)),
        },
        ExprKind::Match { scrutinee, arms } => {
            let as_: Vec<String> = arms
                .iter()
                .map(|a| {
                    let pat = match &a.binder {
                        None => a.tag.clone(),
                        Some(b) => format!("{}({})", a.tag, b),
                    };
                    format!("{} => {}", pat, render_labelled(&a.body, reg))
                })
                .collect();
            format!(
                "match {} {{ {} }}",
                render_labelled(scrutinee, reg),
                as_.join(", ")
            )
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => format!(
            "if {} then {} else {}",
            render_labelled(cond, reg),
            render_labelled(then_branch, reg),
            render_labelled(else_branch, reg)
        ),
        ExprKind::Call { target, args } => {
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
                    | "#c.pow"
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
                    "#c.pow" => "**",
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
        ExprKind::Block { stmts, result } => {
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
