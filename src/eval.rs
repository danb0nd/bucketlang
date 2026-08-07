use crate::ast::{Expr, Stmt};
use crate::error::{Error, Result};
use crate::registry::Registry;
use crate::value::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::Write;

const MAX_CALL_DEPTH: usize = 256;

pub fn eval_bucket(
    reg: &Registry,
    addr: &str,
    args: &[Value],
    out: &mut dyn Write,
) -> Result<Value> {
    eval_bucket_depth(reg, addr, args, out, 0)
}

fn eval_bucket_depth(
    reg: &Registry,
    addr: &str,
    args: &[Value],
    out: &mut dyn Write,
    depth: usize,
) -> Result<Value> {
    if depth > MAX_CALL_DEPTH {
        return Err(Error::msg(format!(
            "call depth exceeded {MAX_CALL_DEPTH} (possible infinite recursion) at {addr}"
        )));
    }
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
    eval_expr(&bucket.body, &mut env, reg, out, addr, depth)
}

fn eval_expr(
    expr: &Expr,
    env: &mut HashMap<String, Value>,
    reg: &Registry,
    out: &mut dyn Write,
    from: &str,
    depth: usize,
) -> Result<Value> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| Error::msg(format!("unbound name '{name}' in {from}"))),
        Expr::List(elems) => {
            let mut vals = Vec::new();
            for e in elems {
                vals.push(eval_expr(e, env, reg, out, from, depth)?);
            }
            Ok(Value::List(vals))
        }
        Expr::Record(fields) => {
            let mut map = BTreeMap::new();
            for (k, v) in fields {
                map.insert(k.clone(), eval_expr(v, env, reg, out, from, depth)?);
            }
            Ok(Value::Record(map))
        }
        Expr::Field { base, field } => {
            let rec = eval_expr(base, env, reg, out, from, depth)?;
            let map = rec.as_record().map_err(Error::msg)?;
            map.get(field)
                .cloned()
                .ok_or_else(|| Error::msg(format!("missing field '{field}' in {from}")))
        }
        Expr::Variant { tag, payload } => {
            let p = match payload {
                None => None,
                Some(e) => Some(Box::new(eval_expr(e, env, reg, out, from, depth)?)),
            };
            Ok(Value::Variant {
                tag: tag.clone(),
                payload: p,
            })
        }
        Expr::Match { scrutinee, arms } => {
            let v = eval_expr(scrutinee, env, reg, out, from, depth)?;
            let Value::Variant { tag, payload } = v else {
                return Err(Error::msg(format!(
                    "match expected variant in {from}, got {}",
                    v.ty().name()
                )));
            };
            for arm in arms {
                if arm.tag == tag {
                    if let Some(binder) = &arm.binder {
                        let p = payload.ok_or_else(|| {
                            Error::msg(format!("match arm '{tag}' expected payload in {from}"))
                        })?;
                        env.insert(binder.clone(), *p);
                    }
                    return eval_expr(&arm.body, env, reg, out, from, depth);
                }
            }
            Err(Error::msg(format!(
                "no match arm for tag '{tag}' in {from}"
            )))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let c = eval_expr(cond, env, reg, out, from, depth)?;
            if c.as_bool().map_err(Error::msg)? {
                eval_expr(then_branch, env, reg, out, from, depth)
            } else {
                eval_expr(else_branch, env, reg, out, from, depth)
            }
        }
        Expr::Call { target, args } => {
            let mut vals = Vec::new();
            for a in args {
                vals.push(eval_expr(a, env, reg, out, from, depth)?);
            }
            eval_call(reg, target, &vals, out, depth)
        }
        Expr::Block { stmts, result } => {
            for stmt in stmts {
                match stmt {
                    Stmt::Bind { name, value } => {
                        let v = eval_expr(value, env, reg, out, from, depth)?;
                        env.insert(name.clone(), v);
                    }
                    Stmt::Run(e) => {
                        let _ = eval_expr(e, env, reg, out, from, depth)?;
                    }
                }
            }
            eval_expr(result, env, reg, out, from, depth)
        }
    }
}

fn eval_call(
    reg: &Registry,
    target: &str,
    args: &[Value],
    out: &mut dyn Write,
    depth: usize,
) -> Result<Value> {
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
        "#c.pow" => {
            let a = args[0].as_num().map_err(Error::msg)?;
            let b = args[1].as_num().map_err(Error::msg)?;
            let r = a.powf(b);
            if r.is_nan() || r.is_infinite() {
                return Err(Error::msg(format!("pow({a}, {b}) is not a finite Num")));
            }
            Ok(Value::Num(r))
        }
        "#c.mod" => {
            let a = args[0].as_num().map_err(Error::msg)?;
            let b = args[1].as_num().map_err(Error::msg)?;
            if b == 0.0 {
                return Err(Error::msg("modulo by zero in #c.mod"));
            }
            Ok(Value::Num(a % b))
        }
        "#c.floor" => Ok(Value::Num(
            args[0].as_num().map_err(Error::msg)?.floor(),
        )),
        "#c.abs" => Ok(Value::Num(args[0].as_num().map_err(Error::msg)?.abs())),
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
        "#c.list_len" => {
            let xs = args[0].as_list().map_err(Error::msg)?;
            Ok(Value::Num(xs.len() as f64))
        }
        "#c.list_nth" => {
            let xs = args[0].as_list().map_err(Error::msg)?;
            let i = args[1].as_num().map_err(Error::msg)?;
            if i.fract() != 0.0 || i < 0.0 {
                return Err(Error::msg(format!("list_nth index must be a non-negative integer, got {i}")));
            }
            let idx = i as usize;
            xs.get(idx)
                .cloned()
                .ok_or_else(|| Error::msg(format!("list_nth index {idx} out of range (len {})", xs.len())))
        }
        "#c.list_append" => {
            let mut xs = args[0].as_list().map_err(Error::msg)?.to_vec();
            xs.push(args[1].clone());
            Ok(Value::List(xs))
        }
        "#c.list_concat" => {
            let a = args[0].as_list().map_err(Error::msg)?;
            let b = args[1].as_list().map_err(Error::msg)?;
            let mut out = a.to_vec();
            out.extend_from_slice(b);
            Ok(Value::List(out))
        }
        "#c.list_remove" => {
            let xs = args[0].as_list().map_err(Error::msg)?;
            let i = args[1].as_num().map_err(Error::msg)?;
            if i.fract() != 0.0 || i < 0.0 {
                return Err(Error::msg(format!(
                    "list_remove index must be a non-negative integer, got {i}"
                )));
            }
            let idx = i as usize;
            if idx >= xs.len() {
                return Err(Error::msg(format!(
                    "list_remove index {idx} out of range (len {})",
                    xs.len()
                )));
            }
            let mut out = xs.to_vec();
            out.remove(idx);
            Ok(Value::List(out))
        }
        other => {
            let bucket = reg
                .get(other)
                .ok_or_else(|| Error::msg(format!("unknown bucket {other}")))?;
            if bucket.kind == crate::ast::BucketKind::Core {
                return Err(Error::msg(format!("unimplemented core {other}")));
            }
            eval_bucket_depth(reg, other, args, out, depth + 1)
        }
    }
}
