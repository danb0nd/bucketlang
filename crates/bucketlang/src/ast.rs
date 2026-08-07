use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Type {
    Num,
    Bool,
    Str,
    List(Box<Type>),
    /// Named fields; order canonicalized by field name.
    Record(BTreeMap<String, Type>),
    /// Tagged union: tag -> optional payload type (`None` = nullary).
    Variant(BTreeMap<String, Option<Type>>),
    /// User type alias (resolved away during compile).
    Name(String),
    /// Type parameter inside a parametric alias RHS (`T` in `Option[T]`).
    Param(String),
    /// Application of a parametric alias: `Option[Num]`.
    App { name: String, args: Vec<Type> },
    /// Core-only (e.g. print accepts any)
    Any,
}

impl Type {
    pub fn name(&self) -> String {
        match self {
            Type::Num => "Num".into(),
            Type::Bool => "Bool".into(),
            Type::Str => "Str".into(),
            Type::List(inner) => format!("List[{}]", inner.name()),
            Type::Record(fields) => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.name()))
                    .collect();
                format!("{{ {} }}", inner.join(", "))
            }
            Type::Variant(tags) => {
                let inner: Vec<String> = tags
                    .iter()
                    .map(|(tag, payload)| match payload {
                        None => tag.clone(),
                        Some(t) => format!("{tag}({})", t.name()),
                    })
                    .collect();
                inner.join(" | ")
            }
            Type::Name(n) => n.clone(),
            Type::Param(n) => n.clone(),
            Type::App { name, args } => {
                let inner: Vec<String> = args.iter().map(|a| a.name()).collect();
                format!("{name}[{}]", inner.join(", "))
            }
            Type::Any => "Any".into(),
        }
    }

    pub fn matches(&self, got: &Type) -> bool {
        match (self, got) {
            (Type::Any, _) | (_, Type::Any) => true,
            (Type::List(a), Type::List(b)) => a.matches(b),
            (Type::Record(a), Type::Record(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                a.iter()
                    .all(|(k, at)| b.get(k).is_some_and(|bt| at.matches(bt)))
            }
            (Type::Variant(expected), Type::Variant(got)) => {
                // Runtime values often carry a single tag; allow got ⊆ expected.
                got.iter().all(|(tag, gp)| {
                    expected.get(tag).is_some_and(|ep| match (ep, gp) {
                        (None, None) => true,
                        (Some(x), Some(y)) => x.matches(y),
                        _ => false,
                    })
                })
            }
            (Type::App { name: n1, args: a1 }, Type::App { name: n2, args: a2 }) => {
                n1 == n2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(x, y)| x.matches(y))
            }
            (a, b) => a == b,
        }
    }

    pub fn list_elem(&self) -> Option<&Type> {
        match self {
            Type::List(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn record_fields(&self) -> Option<&BTreeMap<String, Type>> {
        match self {
            Type::Record(f) => Some(f),
            _ => None,
        }
    }

    pub fn variant_tags(&self) -> Option<&BTreeMap<String, Option<Type>>> {
        match self {
            Type::Variant(t) => Some(t),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Contract {
    pub params: Vec<Param>,
    pub ret: Type,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MatchArm {
    pub tag: String,
    pub binder: Option<String>,
    pub body: Expr,
}

/// An expression plus where it came from.
///
/// The span is what lets a type or runtime error name a line and column instead
/// of only a bucket address. It is carried through name resolution so a resolved
/// body still points back at the source the user wrote.
///
/// `span` is `None` for nodes the compiler synthesizes (desugared `@test` calls,
/// core stubs) — nothing in the source corresponds to them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Expr {
    pub kind: ExprKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Option<Span>) -> Self {
        Expr { kind, span }
    }

    /// A node with no source location — only for compiler-synthesized code.
    pub fn synthetic(kind: ExprKind) -> Self {
        Expr { kind, span: None }
    }

    /// Nearest span at or under this node, for errors about a node that was
    /// itself synthesized but whose children came from real source.
    pub fn any_span(&self) -> Option<Span> {
        if self.span.is_some() {
            return self.span;
        }
        self.children().into_iter().find_map(|c| c.any_span())
    }

    pub fn children(&self) -> Vec<&Expr> {
        match &self.kind {
            ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Var(_) => vec![],
            ExprKind::List(xs) => xs.iter().collect(),
            ExprKind::Record(fs) => fs.iter().map(|(_, e)| e).collect(),
            ExprKind::Field { base, .. } => vec![base.as_ref()],
            ExprKind::Variant { payload, .. } => payload.iter().map(|p| p.as_ref()).collect(),
            ExprKind::Match { scrutinee, arms } => {
                let mut v = vec![scrutinee.as_ref()];
                v.extend(arms.iter().map(|a| &a.body));
                v
            }
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => vec![cond.as_ref(), then_branch.as_ref(), else_branch.as_ref()],
            ExprKind::Call { args, .. } => args.iter().collect(),
            ExprKind::Block { stmts, result } => {
                let mut v: Vec<&Expr> = stmts
                    .iter()
                    .map(|s| match s {
                        Stmt::Bind { value, .. } => value,
                        Stmt::Run(e) => e,
                    })
                    .collect();
                v.push(result.as_ref());
                v
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ExprKind {
    Num(f64),
    Bool(bool),
    Str(String),
    Var(String),
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    Field {
        base: Box<Expr>,
        field: String,
    },
    Variant {
        tag: String,
        payload: Option<Box<Expr>>,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Call {
        target: String,
        args: Vec<Expr>,
    },
    Block {
        stmts: Vec<Stmt>,
        result: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Stmt {
    Bind { name: String, value: Expr },
    Run(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TestAnn {
    pub call_target: String,
    pub args: Vec<Expr>,
    /// Present for `@test call == expected`. Absent for `@test_error call(...)`.
    pub expected: Option<Expr>,
    pub expect_error: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawTypeAlias {
    pub name: String,
    /// Empty = monomorphic alias. Non-empty = parametric (`type Option[T] = …`).
    pub params: Vec<String>,
    pub ty: Type,
}

/// `import util` or `import util::double as dbl`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportDecl {
    pub module: String,
    /// `None` = whole-module import (`import util`).
    pub item: Option<String>,
    /// Local name for an item import. Defaults to `item` when omitted.
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawProgram {
    pub module: Option<String>,
    pub imports: Vec<ImportDecl>,
    pub aliases: Vec<RawTypeAlias>,
    pub buckets: Vec<RawBucket>,
}

/// Byte range of a bucket body inside its own source file, between the braces
/// and excluding them. Only meaningful for the file the bucket was parsed from,
/// which is why linking clears it (see `merge_registry`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawBucket {
    pub explicit_addr: Option<String>,
    pub label: Option<String>,
    pub contract: Contract,
    pub desc: Option<String>,
    pub body: Expr,
    pub is_entry: bool,
    pub tests: Vec<TestAnn>,
    pub body_span: Option<Span>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BucketKind {
    User,
    Test,
    Core,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Complexity {
    pub nodes: usize,
    pub depth: usize,
    pub calls: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bucket {
    pub address: String,
    pub label: Option<String>,
    pub desc: String,
    pub contract: Contract,
    pub body: Expr,
    pub kind: BucketKind,
    pub content_hash: String,
    pub complexity: Complexity,
    pub subject: Option<String>,
    /// Test buckets from `@test_error` — pass iff body evaluation errors.
    pub expect_error: bool,
    /// Where this bucket's body sits in the source it was parsed from.
    /// `None` for cores, synthesized tests, and anything linked in from an
    /// import — for those there is no span into the file being edited.
    pub body_span: Option<Span>,
}
