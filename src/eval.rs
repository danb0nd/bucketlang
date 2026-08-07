use crate::ast::{Expr, Stmt};
use crate::error::{Error, Result};
use crate::registry::Registry;
use crate::value::Value;
use std::collections::HashMap;
use std::io::Write;

pub fn eval_bucket(
    reg: &Registry,
    addr: &str,
    args: &[Value],
    out: &mut dyn Write,
) -> Result<Value> {
    let bucket = reg
        .get(addr)
        .ok_or_else(|| Error::msg(format!("unknown bucket {addr}")))?;
    if args.len() != bucket.contract.params.len() {
        return Err(Error::msg(format!(
            "arity mismatch invoking {}: expected {}, got {}",
            addr,
            bucket.contract.params.len(),
            args.len()
        )));
    }
    let mut env = HashMap::new();
    for (p, v) in bucket.contract.params.iter().zip(args.iter()) {
        if !p.ty.matches(&v.ty()) && p.ty != crate::ast::Type::Any {
            return Err(Error::msg(format!(
                "arg type mismatch for {}: expected {}, got {}",
                p.name,
                p.ty.name(),
                v.ty().name()
            )));
        }
        env.insert(p.name.clone(), v.clone());
    }
    eval_expr(&bucket.body, &mut env, reg, out, addr)
}

fn eval_expr(
    expr: &Expr,
    env: &mut HashMap<String, Value>,
    reg: &Registry,
    out: &mut dyn Write,
    from: &str,
) -> Result<Value> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| Error::msg(format!("unbound name '{name}' in {from}"))),
        Expr::Call { target, args } => {
            let mut vals = Vec::new();
            for a in args {
                vals.push(eval_expr(a, env, reg, out, from)?);
            }
            eval_call(reg, target, &vals, out)
        }
        Expr::Block { stmts, result } => {
            for stmt in stmts {
                match stmt {
                    Stmt::Bind { name, value } => {
                        let v = eval_expr(value, env, reg, out, from)?;
                        env.insert(name.clone(), v);
                    }
                    Stmt::Run(e) => {
                        let _ = eval_expr(e, env, reg, out, from)?;
                    }
                }
            }
            eval_expr(result, env, reg, out, from)
        }
    }
}

fn eval_call(reg: &Registry, target: &str, args: &[Value], out: &mut dyn Write) -> Result<Value> {
    match target {
        "#c.add" => match (&args[0], &args[1]) {
            (Value::Num(a), Value::Num(b)) => Ok(Value::Num(a + b)),
            (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{a}{b}"))),
            _ => Err(Error::msg("#c.add expects Num+Num or Str+Str")),
        },
        "#c.sub" => Ok(Value::Num(
            args[0].as_num().map_err(Error::msg)? - args[1].as_num().map_err(Error::msg)?,
        )),
        "#c.mul" => Ok(Value::Num(
            args[0].as_num().map_err(Error::msg)? * args[1].as_num().map_err(Error::msg)?,
        )),
        "#c.div" => {
            let b = args[1].as_num().map_err(Error::msg)?;
            if b == 0.0 {
                return Err(Error::msg("division by zero in #c.div"));
            }
            Ok(Value::Num(args[0].as_num().map_err(Error::msg)? / b))
        }
        "#c.eq" => Ok(Value::Bool(args[0].equals(&args[1]))),
        "#c.ne" => Ok(Value::Bool(!args[0].equals(&args[1]))),
        "#c.lt" => Ok(Value::Bool(
            args[0].as_num().map_err(Error::msg)? < args[1].as_num().map_err(Error::msg)?,
        )),
        "#c.gt" => Ok(Value::Bool(
            args[0].as_num().map_err(Error::msg)? > args[1].as_num().map_err(Error::msg)?,
        )),
        "#c.le" => Ok(Value::Bool(
            args[0].as_num().map_err(Error::msg)? <= args[1].as_num().map_err(Error::msg)?,
        )),
        "#c.ge" => Ok(Value::Bool(
            args[0].as_num().map_err(Error::msg)? >= args[1].as_num().map_err(Error::msg)?,
        )),
        "#c.and" => Ok(Value::Bool(
            args[0].as_bool().map_err(Error::msg)? && args[1].as_bool().map_err(Error::msg)?,
        )),
        "#c.or" => Ok(Value::Bool(
            args[0].as_bool().map_err(Error::msg)? || args[1].as_bool().map_err(Error::msg)?,
        )),
        "#c.not" => Ok(Value::Bool(!args[0].as_bool().map_err(Error::msg)?)),
        "#c.print" => {
            writeln!(out, "{}", args[0].display())
                .map_err(|e| Error::msg(format!("print failed: {e}")))?;
            let _ = out.flush();
            Ok(args[0].clone())
        }
        "#c.assert_eq" => {
            if args[0].equals(&args[1]) {
                Ok(Value::Bool(true))
            } else {
                Err(Error::msg(format!(
                    "assert_eq failed: {} != {}",
                    args[0].display(),
                    args[1].display()
                )))
            }
        }
        other => {
            let bucket = reg
                .get(other)
                .ok_or_else(|| Error::msg(format!("unknown bucket {other}")))?;
            if bucket.kind == crate::ast::BucketKind::Core {
                return Err(Error::msg(format!("unimplemented core {other}")));
            }
            eval_bucket(reg, other, args, out)
        }
    }
}
