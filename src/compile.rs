use crate::ast::{Bucket, BucketKind, Expr, RawBucket};
use crate::canonical::content_hash;
use crate::complexity::{measure, within_budget};
use crate::error::{Error, Result};
use crate::lexer::tokenize;
use crate::parser::Parser;
use crate::registry::Registry;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy)]
pub struct CompileOptions {
    pub strict: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self { strict: true }
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

            // Polymorphic cores
            match target.as_str() {
                "#c.add" => match (&arg_tys[0], &arg_tys[1]) {
                    (Type::Num, Type::Num) => Ok(Type::Num),
                    (Type::Str, Type::Str) => Ok(Type::Str),
                    _ => Err(Error::msg(format!(
                        "#c.add needs Num+Num or Str+Str (in {bucket_addr})"
                    ))),
                },
                "#c.eq" | "#c.ne" | "#c.assert_eq" => {
                    if arg_tys[0] == arg_tys[1]
                        || arg_tys[0] == Type::Any
                        || arg_tys[1] == Type::Any
                    {
                        Ok(Type::Bool)
                    } else {
                        Err(Error::msg(format!(
                            "{target} needs same-typed args (in {bucket_addr})"
                        )))
                    }
                }
                "#c.print" => Ok(arg_tys[0].clone()),
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
                        if env.contains_key(name) && !env.get(name).unwrap().matches(&ty) {
                            // allow shadowing with same or any
                        }
                        if env.contains_key(name)
                            && env.keys().filter(|k| *k == name).count() > 0
                        {
                            // duplicate local in same block already checked? allow shadow
                        }
                        // duplicate bind in same block: error if already a local from this block
                        // Track only: if we insert twice in this loop without shadow intent
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
