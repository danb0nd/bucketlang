use crate::ast::{
    Bucket, BucketKind, Contract, Expr, ImportDecl, MatchArm, RawBucket, RawProgram, Type,
};
use crate::canonical::content_hash;
use crate::complexity::{measure, within_budget};
use crate::error::{Error, Result};
use crate::lexer::tokenize;
use crate::parser::Parser;
use crate::registry::Registry;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct CtorInfo {
    variant_ty: Type,
    payload: Option<Type>,
}

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
    /// Resolved type aliases (RHS fully expanded).
    pub type_aliases: BTreeMap<String, Type>,
    pub module: Option<String>,
    pub imports: Vec<ImportDecl>,
}

/// Compile a single source string (no `import` resolution).
pub fn compile(source: &str, opts: CompileOptions) -> Result<CompileResult> {
    let mut stack = BTreeSet::new();
    compile_source(source, opts, None, &mut stack)
}

/// Compile source as if it lived at `base_file` (resolves `import`s relative to that path).
pub fn compile_with_base(
    source: &str,
    opts: CompileOptions,
    base_file: &Path,
) -> Result<CompileResult> {
    let mut stack = BTreeSet::new();
    compile_source(source, opts, Some(base_file), &mut stack)
}

/// Compile a `.bkt` file and recursively link `import`s.
pub fn compile_file(path: &Path, opts: CompileOptions) -> Result<CompileResult> {
    let mut stack = BTreeSet::new();
    compile_path(path, opts, &mut stack)
}

fn compile_path(
    path: &Path,
    opts: CompileOptions,
    stack: &mut BTreeSet<PathBuf>,
) -> Result<CompileResult> {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !stack.insert(canon.clone()) {
        return Err(Error::msg(format!(
            "import cycle involving {}",
            path.display()
        )));
    }
    let source = std::fs::read_to_string(path)
        .map_err(|e| Error::msg(format!("read {}: {e}", path.display())))?;
    let result = compile_source(&source, opts, Some(path), stack)?;
    stack.remove(&canon);
    Ok(result)
}

fn resolve_import_path(from_file: &Path, name: &str) -> Result<PathBuf> {
    let dir = from_file.parent().unwrap_or_else(|| Path::new("."));
    let candidates = [
        dir.join(format!("{name}.bkt")),
        dir.join(name).join("mod.bkt"),
        dir.join(name).join(format!("{name}.bkt")),
    ];
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    Err(Error::msg(format!(
        "cannot find module '{name}' for import (tried {}, {}, {})",
        candidates[0].display(),
        candidates[1].display(),
        candidates[2].display()
    )))
}

fn compile_source(
    source: &str,
    opts: CompileOptions,
    path: Option<&Path>,
    stack: &mut BTreeSet<PathBuf>,
) -> Result<CompileResult> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(&tokens);
    let program = parser.parse_program()?;

    if !program.imports.is_empty() && path.is_none() {
        return Err(Error::msg(
            "import requires compiling from a file path (not a raw string)",
        ));
    }

    let mut prelude = Registry::with_cores();
    let mut merged_aliases = BTreeMap::new();
    let mut loaded_modules: BTreeSet<String> = BTreeSet::new();
    for imp in &program.imports {
        let mod_name = &imp.module;
        if loaded_modules.insert(mod_name.clone()) {
            let imp_path = resolve_import_path(path.unwrap(), mod_name)?;
            let imported = compile_path(&imp_path, opts, stack)?;
            match &imported.module {
                Some(m) if m == mod_name => {}
                Some(m) => {
                    return Err(Error::msg(format!(
                        "import '{mod_name}' but file declares module '{m}'"
                    )));
                }
                None => {
                    return Err(Error::msg(format!(
                        "imported file {} must declare `module {mod_name}`",
                        imp_path.display()
                    )));
                }
            }
            merge_registry(&mut prelude, &imported.registry)?;
            for (k, v) in imported.type_aliases {
                if merged_aliases.contains_key(&k) {
                    return Err(Error::msg(format!(
                        "duplicate type alias '{k}' from import '{mod_name}'"
                    )));
                }
                merged_aliases.insert(k, v);
            }
        }
    }

    for imp in &program.imports {
        if let Some(item) = &imp.item {
            let qualified = format!("{}::{item}", imp.module);
            let Some(id) = prelude.label_to_id.get(&qualified).cloned() else {
                return Err(Error::msg(format!(
                    "import '{qualified}': no such label in module '{}'",
                    imp.module
                )));
            };
            let local = imp.alias.clone().unwrap_or_else(|| item.clone());
            if let Some(existing) = prelude.label_to_id.get(&local) {
                if existing != &id {
                    return Err(Error::msg(format!(
                        "import alias '{local}' conflicts with an existing label"
                    )));
                }
            } else {
                prelude.label_to_id.insert(local, id);
            }
        }
    }

    let local_aliases = resolve_aliases(&program)?;
    for (k, v) in local_aliases {
        if merged_aliases.contains_key(&k) {
            return Err(Error::msg(format!(
                "type alias '{k}' conflicts with an imported alias"
            )));
        }
        merged_aliases.insert(k, v);
    }

    let registry = lower(
        &program.buckets,
        &merged_aliases,
        opts,
        program.module.as_deref(),
        prelude,
    )?;

    Ok(CompileResult {
        registry,
        tokens,
        raw_buckets: program.buckets,
        type_aliases: merged_aliases,
        module: program.module,
        imports: program.imports,
    })
}

fn merge_registry(into: &mut Registry, from: &Registry) -> Result<()> {
    for (id, b) in &from.buckets {
        if b.kind == BucketKind::Core {
            continue;
        }
        if into.buckets.contains_key(id) {
            return Err(Error::msg(format!(
                "address collision while linking: {id}"
            )));
        }
        into.buckets.insert(id.clone(), b.clone());
    }
    for (label, id) in &from.label_to_id {
        if into.label_to_id.contains_key(label) {
            // allow same qualified/core label mapping to same id only
            if into.label_to_id.get(label) != Some(id) {
                return Err(Error::msg(format!(
                    "label collision while linking: {label}"
                )));
            }
            continue;
        }
        // Do not re-export bare labels across module boundaries.
        // Importers see `mod::name` (and explicit `import … as` aliases).
        if !label.contains("::") {
            continue;
        }
        into.label_to_id.insert(label.clone(), id.clone());
    }
    for tid in &from.test_ids {
        if !into.test_ids.contains(tid) {
            into.test_ids.push(tid.clone());
        }
    }
    // do not take entry from imports
    Ok(())
}

fn resolve_aliases(program: &RawProgram) -> Result<BTreeMap<String, Type>> {
    let mut raw_map: BTreeMap<String, Type> = BTreeMap::new();
    for a in &program.aliases {
        if raw_map.contains_key(&a.name) {
            return Err(Error::msg(format!("duplicate type alias {}", a.name)));
        }
        raw_map.insert(a.name.clone(), a.ty.clone());
    }
    let mut resolved = BTreeMap::new();
    for name in raw_map.keys() {
        let mut stack = BTreeSet::new();
        let ty = expand_type(&Type::Name(name.clone()), &raw_map, &mut stack)?;
        resolved.insert(name.clone(), ty);
    }
    Ok(resolved)
}

fn expand_type(
    ty: &Type,
    aliases: &BTreeMap<String, Type>,
    stack: &mut BTreeSet<String>,
) -> Result<Type> {
    match ty {
        Type::Num | Type::Bool | Type::Str | Type::Any => Ok(ty.clone()),
        Type::List(inner) => Ok(Type::List(Box::new(expand_type(inner, aliases, stack)?))),
        Type::Record(fields) => {
            let mut out = BTreeMap::new();
            for (k, v) in fields {
                out.insert(k.clone(), expand_type(v, aliases, stack)?);
            }
            Ok(Type::Record(out))
        }
        Type::Variant(tags) => {
            let mut out = BTreeMap::new();
            for (tag, payload) in tags {
                let p = match payload {
                    None => None,
                    Some(t) => Some(expand_type(t, aliases, stack)?),
                };
                out.insert(tag.clone(), p);
            }
            Ok(Type::Variant(out))
        }
        Type::Name(name) => {
            if !stack.insert(name.clone()) {
                return Err(Error::msg(format!(
                    "cyclic type alias involving {name}"
                )));
            }
            let raw = aliases.get(name).ok_or_else(|| {
                Error::msg(format!("unknown type alias {name}"))
            })?;
            let expanded = expand_type(raw, aliases, stack)?;
            stack.remove(name);
            Ok(expanded)
        }
    }
}

fn collect_ctors(aliases: &BTreeMap<String, Type>) -> Result<BTreeMap<String, CtorInfo>> {
    let mut ctors = BTreeMap::new();
    for (alias_name, ty) in aliases {
        if let Type::Variant(tags) = ty {
            for (tag, payload) in tags {
                if ctors.contains_key(tag) {
                    return Err(Error::msg(format!(
                        "variant tag '{tag}' reused (must be unique; used by {alias_name})"
                    )));
                }
                ctors.insert(
                    tag.clone(),
                    CtorInfo {
                        variant_ty: ty.clone(),
                        payload: payload.clone(),
                    },
                );
            }
        }
    }
    Ok(ctors)
}

fn expand_contract(c: &Contract, aliases: &BTreeMap<String, Type>) -> Result<Contract> {
    let mut stack = BTreeSet::new();
    let mut params = Vec::new();
    for p in &c.params {
        params.push(crate::ast::Param {
            name: p.name.clone(),
            ty: expand_type(&p.ty, aliases, &mut stack)?,
        });
    }
    Ok(Contract {
        params,
        ret: expand_type(&c.ret, aliases, &mut stack)?,
    })
}

fn lower(
    raw: &[RawBucket],
    aliases: &BTreeMap<String, Type>,
    opts: CompileOptions,
    module: Option<&str>,
    mut reg: Registry,
) -> Result<Registry> {
    let ctors = collect_ctors(aliases)?;
    let mut next_b: u64 = 1;
    let mut next_t: u64 = 1;
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut allocated: Vec<(String, RawBucket)> = Vec::new();

    // Claim addresses already present from imports
    for id in reg.buckets.keys() {
        claimed.insert(id.clone());
    }

    let entry_count = raw.iter().filter(|b| b.is_entry).count();
    if entry_count > 1 {
        return Err(Error::msg("multiple @entry annotations"));
    }

    let mint_user = |n: u64| -> String {
        match module {
            Some(m) => format!("#{m}::b{n:08x}"),
            None => format!("#b{n:08x}"),
        }
    };
    let mint_test = |n: u64| -> String {
        match module {
            Some(m) => format!("#{m}::t{n:08x}"),
            None => format!("#t{n:08x}"),
        }
    };

    for rb in raw {
        let mut rb = rb.clone();
        rb.contract = expand_contract(&rb.contract, aliases)?;
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
            let normalized = normalize_manual_addr(a, module)?;
            if claimed.contains(&normalized) || reg.buckets.contains_key(&normalized) {
                return Err(Error::msg(format!("address already taken: {normalized}")));
            }
            claimed.insert(normalized.clone());
            normalized
        } else {
            loop {
                let cand = mint_user(next_b);
                next_b += 1;
                if !claimed.contains(&cand) {
                    claimed.insert(cand.clone());
                    break cand;
                }
            }
        };

        if let Some(label) = &rb.label {
            reg.label_to_id.insert(label.clone(), addr.clone());
            if let Some(m) = module {
                let q = format!("{m}::{label}");
                reg.label_to_id.insert(q, addr.clone());
            }
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
        let body = resolve_expr(&rb.body, &reg, &known, &ctors)?;
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
                let tid = mint_test(next_t);
                next_t += 1;
                known.insert(tid.clone());

                let call_target = resolve_name(&test.call_target, &reg, &known)?;
                let mut args = Vec::new();
                for a in &test.args {
                    args.push(resolve_expr(a, &reg, &known, &ctors)?);
                }
                let expected = resolve_expr(&test.expected, &reg, &known, &ctors)?;
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
        let got = infer_type(&b.body, &env, &reg, &b.address, &ctors)?;
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

fn normalize_manual_addr(addr: &str, module: Option<&str>) -> Result<String> {
    if addr.contains('/') {
        return Err(Error::msg(format!(
            "manual address must use :: for modules (e.g. #mod::b00000001), got {addr}"
        )));
    }
    match module {
        None => {
            if addr.starts_with("#b") && !addr.contains("::") {
                Ok(addr.to_string())
            } else {
                Err(Error::msg(format!(
                    "manual address must look like #b…, got {addr}"
                )))
            }
        }
        Some(m) => {
            let prefix = format!("#{m}::b");
            if addr.starts_with(&prefix) {
                Ok(addr.to_string())
            } else if addr.starts_with("#b") && !addr.contains("::") {
                Ok(format!("#{m}::{}", &addr[1..]))
            } else {
                Err(Error::msg(format!(
                    "manual address in module {m} must be #b… or #{m}::b…, got {addr}"
                )))
            }
        }
    }
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

fn resolve_expr(
    expr: &Expr,
    reg: &Registry,
    known: &BTreeSet<String>,
    ctors: &BTreeMap<String, CtorInfo>,
) -> Result<Expr> {
    match expr {
        Expr::Num(n) => Ok(Expr::Num(*n)),
        Expr::Bool(b) => Ok(Expr::Bool(*b)),
        Expr::Str(s) => Ok(Expr::Str(s.clone())),
        Expr::Var(p) => {
            if let Some(c) = ctors.get(p) {
                if c.payload.is_none() {
                    return Ok(Expr::Variant {
                        tag: p.clone(),
                        payload: None,
                    });
                }
                return Err(Error::msg(format!(
                    "variant '{p}' needs a payload: {p}(...)"
                )));
            }
            Ok(Expr::Var(p.clone()))
        }
        Expr::List(elems) => {
            let mut out = Vec::new();
            for e in elems {
                out.push(resolve_expr(e, reg, known, ctors)?);
            }
            Ok(Expr::List(out))
        }
        Expr::Record(fields) => {
            let mut out = Vec::new();
            for (k, v) in fields {
                out.push((k.clone(), resolve_expr(v, reg, known, ctors)?));
            }
            Ok(Expr::Record(out))
        }
        Expr::Field { base, field } => Ok(Expr::Field {
            base: Box::new(resolve_expr(base, reg, known, ctors)?),
            field: field.clone(),
        }),
        Expr::Variant { tag, payload } => Ok(Expr::Variant {
            tag: tag.clone(),
            payload: match payload {
                None => None,
                Some(p) => Some(Box::new(resolve_expr(p, reg, known, ctors)?)),
            },
        }),
        Expr::Match { scrutinee, arms } => {
            let mut out_arms = Vec::new();
            for a in arms {
                out_arms.push(MatchArm {
                    tag: a.tag.clone(),
                    binder: a.binder.clone(),
                    body: resolve_expr(&a.body, reg, known, ctors)?,
                });
            }
            Ok(Expr::Match {
                scrutinee: Box::new(resolve_expr(scrutinee, reg, known, ctors)?),
                arms: out_arms,
            })
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => Ok(Expr::If {
            cond: Box::new(resolve_expr(cond, reg, known, ctors)?),
            then_branch: Box::new(resolve_expr(then_branch, reg, known, ctors)?),
            else_branch: Box::new(resolve_expr(else_branch, reg, known, ctors)?),
        }),
        Expr::Call { target, args } => {
            if let Some(c) = ctors.get(target) {
                match &c.payload {
                    Some(_) => {
                        if args.len() != 1 {
                            return Err(Error::msg(format!(
                                "variant '{target}' expects 1 payload arg, got {}",
                                args.len()
                            )));
                        }
                        return Ok(Expr::Variant {
                            tag: target.clone(),
                            payload: Some(Box::new(resolve_expr(&args[0], reg, known, ctors)?)),
                        });
                    }
                    None => {
                        return Err(Error::msg(format!(
                            "variant '{target}' takes no payload (write `{target}`, not `{target}()`)"
                        )));
                    }
                }
            }
            let resolved = resolve_name(target, reg, known)?;
            let mut out_args = Vec::new();
            for a in args {
                out_args.push(resolve_expr(a, reg, known, ctors)?);
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
                            value: resolve_expr(value, reg, known, ctors)?,
                        });
                    }
                    crate::ast::Stmt::Run(e) => {
                        out_s.push(crate::ast::Stmt::Run(resolve_expr(
                            e, reg, known, ctors,
                        )?));
                    }
                }
            }
            Ok(Expr::Block {
                stmts: out_s,
                result: Box::new(resolve_expr(result, reg, known, ctors)?),
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
        Expr::Variant { payload, .. } => payload.as_ref().is_some_and(|p| contains_print(p)),
        Expr::Match { scrutinee, arms } => {
            contains_print(scrutinee) || arms.iter().any(|a| contains_print(&a.body))
        }
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
    ctors: &BTreeMap<String, CtorInfo>,
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
            let mut elem_ty = infer_type(&elems[0], env, reg, bucket_addr, ctors)?;
            for e in &elems[1..] {
                let t = infer_type(e, env, reg, bucket_addr, ctors)?;
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
                let ty = infer_type(v, env, reg, bucket_addr, ctors)?;
                map.insert(k.clone(), ty);
            }
            Ok(Type::Record(map))
        }
        Expr::Field { base, field } => {
            let bt = infer_type(base, env, reg, bucket_addr, ctors)?;
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
        Expr::Variant { tag, payload } => {
            let info = ctors.get(tag).ok_or_else(|| {
                Error::msg(format!("unknown variant tag '{tag}' (in {bucket_addr})"))
            })?;
            match (&info.payload, payload) {
                (None, None) => {}
                (Some(expected), Some(p)) => {
                    let got = infer_type(p, env, reg, bucket_addr, ctors)?;
                    if !expected.matches(&got) {
                        return Err(Error::msg(format!(
                            "variant '{tag}' payload type mismatch: expected {}, got {} (in {bucket_addr})",
                            expected.name(),
                            got.name()
                        )));
                    }
                }
                (None, Some(_)) => {
                    return Err(Error::msg(format!(
                        "variant '{tag}' takes no payload (in {bucket_addr})"
                    )));
                }
                (Some(_), None) => {
                    return Err(Error::msg(format!(
                        "variant '{tag}' needs a payload (in {bucket_addr})"
                    )));
                }
            }
            Ok(info.variant_ty.clone())
        }
        Expr::Match { scrutinee, arms } => {
            let st = infer_type(scrutinee, env, reg, bucket_addr, ctors)?;
            let tags = st.variant_tags().ok_or_else(|| {
                Error::msg(format!(
                    "match on non-variant {} (in {bucket_addr})",
                    st.name()
                ))
            })?;
            let mut seen = BTreeSet::new();
            let mut result_ty: Option<Type> = None;
            for arm in arms {
                if !tags.contains_key(&arm.tag) {
                    return Err(Error::msg(format!(
                        "unknown match tag '{}' for {} (in {bucket_addr})",
                        arm.tag,
                        st.name()
                    )));
                }
                if !seen.insert(arm.tag.clone()) {
                    return Err(Error::msg(format!(
                        "duplicate match arm '{}' (in {bucket_addr})",
                        arm.tag
                    )));
                }
                let payload_ty = tags.get(&arm.tag).unwrap();
                let mut env = env.clone();
                match (payload_ty, &arm.binder) {
                    (None, None) => {}
                    (Some(pt), Some(b)) => {
                        env.insert(b.clone(), pt.clone());
                    }
                    (None, Some(_)) => {
                        return Err(Error::msg(format!(
                            "tag '{}' takes no payload (in {bucket_addr})",
                            arm.tag
                        )));
                    }
                    (Some(_), None) => {
                        return Err(Error::msg(format!(
                            "tag '{}' needs a binder like {}(x) (in {bucket_addr})",
                            arm.tag, arm.tag
                        )));
                    }
                }
                let bt = infer_type(&arm.body, &env, reg, bucket_addr, ctors)?;
                match &result_ty {
                    None => result_ty = Some(bt),
                    Some(prev) => {
                        if !prev.matches(&bt) && !bt.matches(prev) {
                            return Err(Error::msg(format!(
                                "match arms type mismatch {} vs {} (in {bucket_addr})",
                                prev.name(),
                                bt.name()
                            )));
                        }
                    }
                }
            }
            for tag in tags.keys() {
                if !seen.contains(tag) {
                    return Err(Error::msg(format!(
                        "non-exhaustive match: missing '{tag}' (in {bucket_addr})"
                    )));
                }
            }
            result_ty.ok_or_else(|| Error::msg(format!("empty match (in {bucket_addr})")))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let ct = infer_type(cond, env, reg, bucket_addr, ctors)?;
            if ct != Type::Bool && ct != Type::Any {
                return Err(Error::msg(format!(
                    "if condition must be Bool, got {} (in {bucket_addr})",
                    ct.name()
                )));
            }
            let t1 = infer_type(then_branch, env, reg, bucket_addr, ctors)?;
            let t2 = infer_type(else_branch, env, reg, bucket_addr, ctors)?;
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
                arg_tys.push(infer_type(a, env, reg, bucket_addr, ctors)?);
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
                        let ty = infer_type(value, &env, reg, bucket_addr, ctors)?;
                        env.insert(name.clone(), ty);
                    }
                    crate::ast::Stmt::Run(e) => {
                        let _ = infer_type(e, &env, reg, bucket_addr, ctors)?;
                    }
                }
            }
            infer_type(result, &env, reg, bucket_addr, ctors)
        }
    }
}
