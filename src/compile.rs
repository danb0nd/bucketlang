use crate::ast::{Bucket, BucketKind, Expr, RawBucket};
use crate::canonical::content_hash;
use crate::complexity::{measure, within_budget};
use crate::error::{Error, Result};
use crate::lexer::tokenize;
use crate::parser::Parser;
use crate::registry::Registry;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    /// Dev / test: lower `@test` into `#t…` shadow buckets and run them.
    Dev,
    /// Release / final: strip tests from the compiled program.
    Release,
}

#[derive(Debug, Clone, Copy)]
pub struct CompileOptions {
    pub strict: bool,
    pub profile: BuildProfile,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            strict: true,
            profile: BuildProfile::Dev,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CompileResult {
    pub registry: Registry,
    pub tokens: Vec<crate::lexer::Token>,
    pub raw_buckets: Vec<RawBucket>,
}

pub fn compile(source: &str, opts: CompileOptions) -> Result<CompileResult> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(&tokens);
    let raw_buckets = parser.parse_program()?;
    let registry = lower(&raw_buckets, opts)?;
    Ok(CompileResult {
        registry,
        tokens,
        raw_buckets,
    })
}

fn lower(raw: &[RawBucket], opts: CompileOptions) -> Result<Registry> {
    let mut reg = Registry::with_cores();
    let mut next_b: u64 = 1;
    let mut next_t: u64 = 1;
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut allocated: Vec<(String, RawBucket)> = Vec::new();

    let entry_count = raw.iter().filter(|b| b.is_entry).count();
    if entry_count > 1 {
        return Err(Error::msg("multiple @entry annotations"));
    }

    for rb in raw {
        if opts.strict {
            if rb.label.is_none() {
                return Err(Error::msg("strict mode requires a bucket label"));
            }
            match &rb.desc {
                None => {
                    return Err(Error::msg(format!(
                        "strict mode requires a description string after the return type (bucket {})",
                        rb.label.as_deref().unwrap_or("?")
                    )));
                }
                Some(d) if d.trim().is_empty() => {
                    return Err(Error::msg("strict mode requires a non-empty description"));
                }
                _ => {}
            }
        }

        if let Some(label) = &rb.label {
            if reg.label_to_id.contains_key(label) {
                return Err(Error::msg(format!("duplicate label: {label}")));
            }
        }

        let addr = if let Some(a) = &rb.explicit_addr {
            if !a.starts_with("#b") {
                return Err(Error::msg(format!(
                    "manual address must be in #b space, got {a}"
                )));
            }
            if claimed.contains(a) || reg.buckets.contains_key(a) {
                return Err(Error::msg(format!("address already taken: {a}")));
            }
            claimed.insert(a.clone());
            a.clone()
        } else {
            loop {
                let cand = format!("#b{next_b:08x}");
                next_b += 1;
                if !claimed.contains(&cand) {
                    claimed.insert(cand.clone());
                    break cand;
                }
            }
        };

        if let Some(label) = &rb.label {
            reg.label_to_id.insert(label.clone(), addr.clone());
        }
        if rb.is_entry {
            if opts.strict && (rb.label.is_none() || rb.desc.as_ref().map(|d| d.trim().is_empty()).unwrap_or(true))
            {
                return Err(Error::msg(
                    "strict mode: @entry bucket must be labelled and described",
                ));
            }
            reg.entry = Some(addr.clone());
        }
        allocated.push((addr, rb.clone()));
    }

    // Known ids for resolution (cores + allocated user addrs)
    let mut known = claimed.clone();
    for id in reg.buckets.keys() {
        known.insert(id.clone());
    }

    for (addr, rb) in &allocated {
        let body = resolve_expr(&rb.body, &reg, &known)?;
        let complexity = measure(&body);
        within_budget(&complexity).map_err(|e| {
            Error::msg(format!(
                "{e} in bucket {}",
                rb.label.as_deref().unwrap_or(addr)
            ))
        })?;

        let mut bucket = Bucket {
            address: addr.clone(),
            label: rb.label.clone(),
            desc: rb.desc.clone().unwrap_or_default(),
            contract: rb.contract.clone(),
            body: body.clone(),
            kind: BucketKind::User,
            content_hash: content_hash(&body),
            complexity,
            subject: None,
        };
        // silence unused mut warning path
        let _ = &mut bucket;
        reg.buckets.insert(addr.clone(), bucket);
    }

    // Shadow tests only in Dev profile (Release strips @test from the program)
    if opts.profile == BuildProfile::Dev {
        for (addr, rb) in &allocated {
            let subject = rb.label.clone().unwrap_or_else(|| addr.clone());
            for test in &rb.tests {
                let tid = format!("#t{next_t:08x}");
                next_t += 1;
                known.insert(tid.clone());

                let call_target = resolve_name(&test.call_target, &reg, &known)?;
                let mut args = Vec::new();
                for a in &test.args {
                    args.push(resolve_expr(a, &reg, &known)?);
                }
                let expected = resolve_expr(&test.expected, &reg, &known)?;
                let body = Expr::Call {
                    target: "#c.assert_eq".into(),
                    args: vec![
                        Expr::Call {
                            target: call_target,
                            args,
                        },
                        expected,
                    ],
                };
                if contains_print(&body) {
                    return Err(Error::msg("print / #c.print is forbidden inside @test"));
                }
                let complexity = measure(&body);
                within_budget(&complexity).map_err(Error::msg)?;
                let bucket = Bucket {
                    address: tid.clone(),
                    label: None,
                    desc: format!("test: {} == …", test.call_target),
                    contract: crate::ast::Contract {
                        params: vec![],
                        ret: crate::ast::Type::Bool,
                    },
                    body: body.clone(),
                    kind: BucketKind::Test,
                    content_hash: content_hash(&body),
                    complexity,
                    subject: Some(subject.clone()),
                };
                reg.test_ids.push(tid.clone());
                reg.buckets.insert(tid, bucket);
            }
        }
    }

    for b in reg.buckets.values() {
        if b.kind == BucketKind::Core {
            continue;
        }
        let mut env = std::collections::HashMap::new();
        for p in &b.contract.params {
            env.insert(p.name.clone(), p.ty.clone());
        }
        let got = infer_type(&b.body, &env, &reg, &b.address)?;
        if b.kind == BucketKind::User {
            let ret = &b.contract.ret;
            if !ret.matches(&got) && got != crate::ast::Type::Any {
                return Err(Error::msg(format!(
                    "body type {} does not match declared return {} in {}",
                    got.name(),
                    ret.name(),
                    b.address
                )));
            }
        }
    }

    Ok(reg)
}

fn resolve_name(target: &str, reg: &Registry, known: &BTreeSet<String>) -> Result<String> {
    if target.starts_with('#') {
        if known.contains(target) || reg.buckets.contains_key(target) {
            Ok(target.to_string())
        } else {
            Err(Error::msg(format!("unknown address {target}")))
        }
    } else if let Some(id) = reg.label_to_id.get(target) {
        Ok(id.clone())
    } else {
        Err(Error::msg(format!("unknown call target: {target}")))
    }
}

fn resolve_expr(expr: &Expr, reg: &Registry, known: &BTreeSet<String>) -> Result<Expr> {
    match expr {
        Expr::Num(n) => Ok(Expr::Num(*n)),
        Expr::Bool(b) => Ok(Expr::Bool(*b)),
        Expr::Str(s) => Ok(Expr::Str(s.clone())),
        Expr::Var(p) => Ok(Expr::Var(p.clone())),
        Expr::List(elems) => {
            let mut out = Vec::new();
            for e in elems {
                out.push(resolve_expr(e, reg, known)?);
            }
            Ok(Expr::List(out))
        }
        Expr::Record(fields) => {
            let mut out = Vec::new();
            for (k, v) in fields {
                out.push((k.clone(), resolve_expr(v, reg, known)?));
            }
            Ok(Expr::Record(out))
        }
        Expr::Field { base, field } => Ok(Expr::Field {
            base: Box::new(resolve_expr(base, reg, known)?),
            field: field.clone(),
        }),
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => Ok(Expr::If {
            cond: Box::new(resolve_expr(cond, reg, known)?),
            then_branch: Box::new(resolve_expr(then_branch, reg, known)?),
            else_branch: Box::new(resolve_expr(else_branch, reg, known)?),
        }),
        Expr::Call { target, args } => {
            let resolved = resolve_name(target, reg, known)?;
            let mut out_args = Vec::new();
            for a in args {
                out_args.push(resolve_expr(a, reg, known)?);
            }
            Ok(Expr::Call {
                target: resolved,
                args: out_args,
            })
        }
        Expr::Block { stmts, result } => {
            let mut out_s = Vec::new();
            for s in stmts {
                match s {
                    crate::ast::Stmt::Bind { name, value } => {
                        out_s.push(crate::ast::Stmt::Bind {
                            name: name.clone(),
                            value: resolve_expr(value, reg, known)?,
                        });
                    }
                    crate::ast::Stmt::Run(e) => {
                        out_s.push(crate::ast::Stmt::Run(resolve_expr(e, reg, known)?));
                    }
                }
            }
            Ok(Expr::Block {
                stmts: out_s,
                result: Box::new(resolve_expr(result, reg, known)?),
            })
        }
    }
}

fn contains_print(expr: &Expr) -> bool {
    match expr {
        Expr::Call { target, args } => {
            (target == "#c.print" || target == "print") || args.iter().any(contains_print)
        }
        Expr::List(elems) => elems.iter().any(contains_print),
        Expr::Record(fields) => fields.iter().any(|(_, v)| contains_print(v)),
        Expr::Field { base, .. } => contains_print(base),
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => contains_print(cond) || contains_print(then_branch) || contains_print(else_branch),
        Expr::Block { stmts, result } => {
            stmts.iter().any(|s| match s {
                crate::ast::Stmt::Bind { value, .. } => contains_print(value),
                crate::ast::Stmt::Run(e) => contains_print(e),
            }) || contains_print(result)
        }
        _ => false,
    }
}

fn infer_type(
    expr: &Expr,
    env: &std::collections::HashMap<String, crate::ast::Type>,
    reg: &Registry,
    bucket_addr: &str,
) -> Result<crate::ast::Type> {
    use crate::ast::Type;
    match expr {
        Expr::Num(_) => Ok(Type::Num),
        Expr::Bool(_) => Ok(Type::Bool),
        Expr::Str(_) => Ok(Type::Str),
        Expr::Var(name) => env.get(name).cloned().ok_or_else(|| {
            Error::msg(format!("unknown name '{name}' in {bucket_addr}"))
        }),
        Expr::List(elems) => {
            if elems.is_empty() {
                return Ok(Type::List(Box::new(Type::Any)));
            }
            let mut elem_ty = infer_type(&elems[0], env, reg, bucket_addr)?;
            for e in &elems[1..] {
                let t = infer_type(e, env, reg, bucket_addr)?;
                if !elem_ty.matches(&t) {
                    return Err(Error::msg(format!(
                        "heterogeneous list elements {} vs {} (in {bucket_addr})",
                        elem_ty.name(),
                        t.name()
                    )));
                }
                if elem_ty == Type::Any {
                    elem_ty = t;
                }
            }
            Ok(Type::List(Box::new(elem_ty)))
        }
        Expr::Record(fields) => {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in fields {
                let ty = infer_type(v, env, reg, bucket_addr)?;
                map.insert(k.clone(), ty);
            }
            Ok(Type::Record(map))
        }
        Expr::Field { base, field } => {
            let bt = infer_type(base, env, reg, bucket_addr)?;
            let fields = bt.record_fields().ok_or_else(|| {
                Error::msg(format!(
                    "field access on non-record {} (in {bucket_addr})",
                    bt.name()
                ))
            })?;
            fields.get(field).cloned().ok_or_else(|| {
                Error::msg(format!(
                    "unknown field '{field}' on {} (in {bucket_addr})",
                    bt.name()
                ))
            })
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let ct = infer_type(cond, env, reg, bucket_addr)?;
            if ct != Type::Bool && ct != Type::Any {
                return Err(Error::msg(format!(
                    "if condition must be Bool, got {} (in {bucket_addr})",
                    ct.name()
                )));
            }
            let t1 = infer_type(then_branch, env, reg, bucket_addr)?;
            let t2 = infer_type(else_branch, env, reg, bucket_addr)?;
            if t1.matches(&t2) || t2.matches(&t1) {
                if t1 == Type::Any {
                    Ok(t2)
                } else {
                    Ok(t1)
                }
            } else {
                Err(Error::msg(format!(
                    "if branches must match: {} vs {} (in {bucket_addr})",
                    t1.name(),
                    t2.name()
                )))
            }
        }
        Expr::Call { target, args } => {
            let callee = reg.get(target).ok_or_else(|| {
                Error::msg(format!(
                    "call to unknown bucket {target} from {bucket_addr}"
                ))
            })?;
            if args.len() != callee.contract.params.len() {
                return Err(Error::msg(format!(
                    "arity mismatch calling {}: expected {}, got {} (in {})",
                    target,
                    callee.contract.params.len(),
                    args.len(),
                    bucket_addr
                )));
            }
            let mut arg_tys = Vec::new();
            for a in args {
                arg_tys.push(infer_type(a, env, reg, bucket_addr)?);
            }

            match target.as_str() {
                "#c.add" => match (&arg_tys[0], &arg_tys[1]) {
                    (Type::Num, Type::Num) => Ok(Type::Num),
                    (Type::Str, Type::Str) => Ok(Type::Str),
                    _ => Err(Error::msg(format!(
                        "#c.add needs Num+Num or Str+Str (in {bucket_addr})"
                    ))),
                },
                "#c.eq" | "#c.ne" | "#c.assert_eq" => {
                    if arg_tys[0].matches(&arg_tys[1]) {
                        Ok(Type::Bool)
                    } else {
                        Err(Error::msg(format!(
                            "{target} needs same-typed args (in {bucket_addr})"
                        )))
                    }
                }
                "#c.print" => Ok(arg_tys[0].clone()),
                "#c.list_len" => {
                    if arg_tys[0].list_elem().is_none() && arg_tys[0] != Type::Any {
                        return Err(Error::msg(format!(
                            "list_len expects List, got {} (in {bucket_addr})",
                            arg_tys[0].name()
                        )));
                    }
                    Ok(Type::Num)
                }
                "#c.list_nth" => {
                    let elem = arg_tys[0].list_elem().cloned().ok_or_else(|| {
                        Error::msg(format!(
                            "list_nth expects List, got {} (in {bucket_addr})",
                            arg_tys[0].name()
                        ))
                    })?;
                    if arg_tys[1] != Type::Num && arg_tys[1] != Type::Any {
                        return Err(Error::msg(format!(
                            "list_nth index must be Num (in {bucket_addr})"
                        )));
                    }
                    Ok(elem)
                }
                "#c.list_append" => {
                    let elem = arg_tys[0].list_elem().cloned().ok_or_else(|| {
                        Error::msg(format!(
                            "list_append expects List, got {} (in {bucket_addr})",
                            arg_tys[0].name()
                        ))
                    })?;
                    if !elem.matches(&arg_tys[1]) {
                        return Err(Error::msg(format!(
                            "list_append element type mismatch (in {bucket_addr})"
                        )));
                    }
                    let out_elem = if elem == Type::Any {
                        arg_tys[1].clone()
                    } else {
                        elem
                    };
                    Ok(Type::List(Box::new(out_elem)))
                }
                "#c.list_concat" => {
                    let e0 = arg_tys[0].list_elem().cloned().ok_or_else(|| {
                        Error::msg(format!("list_concat expects List (in {bucket_addr})"))
                    })?;
                    let e1 = arg_tys[1].list_elem().cloned().ok_or_else(|| {
                        Error::msg(format!("list_concat expects List (in {bucket_addr})"))
                    })?;
                    if !e0.matches(&e1) {
                        return Err(Error::msg(format!(
                            "list_concat element type mismatch (in {bucket_addr})"
                        )));
                    }
                    let out = if e0 == Type::Any { e1 } else { e0 };
                    Ok(Type::List(Box::new(out)))
                }
                "#c.list_remove" => {
                    let elem = arg_tys[0].list_elem().cloned().ok_or_else(|| {
                        Error::msg(format!(
                            "list_remove expects List, got {} (in {bucket_addr})",
                            arg_tys[0].name()
                        ))
                    })?;
                    if arg_tys[1] != Type::Num && arg_tys[1] != Type::Any {
                        return Err(Error::msg(format!(
                            "list_remove index must be Num (in {bucket_addr})"
                        )));
                    }
                    Ok(Type::List(Box::new(elem)))
                }
                _ => {
                    for (i, pty) in callee.contract.params.iter().enumerate() {
                        if !pty.ty.matches(&arg_tys[i]) {
                            return Err(Error::msg(format!(
                                "type mismatch calling {}: arg {} expected {}, got {} (in {})",
                                target,
                                i,
                                pty.ty.name(),
                                arg_tys[i].name(),
                                bucket_addr
                            )));
                        }
                    }
                    if callee.contract.ret == Type::Any {
                        Ok(arg_tys.first().cloned().unwrap_or(Type::Any))
                    } else {
                        Ok(callee.contract.ret.clone())
                    }
                }
            }
        }
        Expr::Block { stmts, result } => {
            let mut env = env.clone();
            for s in stmts {
                match s {
                    crate::ast::Stmt::Bind { name, value } => {
                        let ty = infer_type(value, &env, reg, bucket_addr)?;
                        env.insert(name.clone(), ty);
                    }
                    crate::ast::Stmt::Run(e) => {
                        let _ = infer_type(e, &env, reg, bucket_addr)?;
                    }
                }
            }
            infer_type(result, &env, reg, bucket_addr)
        }
    }
}
