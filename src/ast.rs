use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Type {
    Num,
    Bool,
    Str,
    /// Core-only (e.g. print accepts any)
    Any,
}

impl Type {
    pub fn name(&self) -> &'static str {
        match self {
            Type::Num => "Num",
            Type::Bool => "Bool",
            Type::Str => "Str",
            Type::Any => "Any",
        }
    }

    pub fn matches(&self, got: &Type) -> bool {
        matches!(self, Type::Any) || self == got
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
pub enum Expr {
    Num(f64),
    Bool(bool),
    Str(String),
    Var(String),
    Call { target: String, args: Vec<Expr> },
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

// (types live here; values in crate::value)
