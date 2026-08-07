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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Expr {
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
    pub expected: Expr,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawTypeAlias {
    pub name: String,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawBucket {
    pub explicit_addr: Option<String>,
    pub label: Option<String>,
    pub contract: Contract,
    pub desc: Option<String>,
    pub body: Expr,
    pub is_entry: bool,
    pub tests: Vec<TestAnn>,
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
}
