use crate::ast::{Bucket, BucketKind, Complexity, Contract, Expr, Param, Type};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct Registry {
    pub buckets: BTreeMap<String, Bucket>,
    pub label_to_id: BTreeMap<String, String>,
    pub entry: Option<String>,
    pub test_ids: Vec<String>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            buckets: BTreeMap::new(),
            label_to_id: BTreeMap::new(),
            entry: None,
            test_ids: Vec::new(),
        }
    }

    pub fn with_cores() -> Self {
        let mut reg = Self::new();
        reg.install_cores();
        reg
    }

    fn core(
        &mut self,
        addr: &str,
        label: Option<&str>,
        desc: &str,
        params: Vec<(&str, Type)>,
        ret: Type,
    ) {
        let bucket = Bucket {
            address: addr.into(),
            label: label.map(|s| s.into()),
            desc: desc.into(),
            contract: Contract {
                params: params
                    .into_iter()
                    .map(|(n, ty)| Param {
                        name: n.into(),
                        ty,
                    })
                    .collect(),
                ret,
            },
            body: Expr::Num(0.0),
            kind: BucketKind::Core,
            content_hash: "core".into(),
            complexity: Complexity {
                nodes: 0,
                depth: 0,
                calls: 0,
            },
            subject: None,
        };
        if let Some(l) = label {
            self.label_to_id.insert(l.into(), addr.into());
        }
        self.buckets.insert(addr.into(), bucket);
    }

    fn install_cores(&mut self) {
        let n2 = |a, b| vec![(a, Type::Num), (b, Type::Num)];
        self.core("#c.add", None, "add / concat", n2("a", "b"), Type::Any);
        self.core("#c.sub", None, "subtract", n2("a", "b"), Type::Num);
        self.core("#c.mul", None, "multiply", n2("a", "b"), Type::Num);
        self.core("#c.div", None, "divide", n2("a", "b"), Type::Num);

        self.core(
            "#c.eq",
            None,
            "equal",
            vec![("a", Type::Any), ("b", Type::Any)],
            Type::Bool,
        );
        self.core(
            "#c.ne",
            None,
            "not equal",
            vec![("a", Type::Any), ("b", Type::Any)],
            Type::Bool,
        );
        self.core("#c.lt", None, "less than", n2("a", "b"), Type::Bool);
        self.core("#c.gt", None, "greater than", n2("a", "b"), Type::Bool);
        self.core("#c.le", None, "less or equal", n2("a", "b"), Type::Bool);
        self.core("#c.ge", None, "greater or equal", n2("a", "b"), Type::Bool);

        self.core(
            "#c.and",
            None,
            "logical and",
            vec![("a", Type::Bool), ("b", Type::Bool)],
            Type::Bool,
        );
        self.core(
            "#c.or",
            None,
            "logical or",
            vec![("a", Type::Bool), ("b", Type::Bool)],
            Type::Bool,
        );
        self.core(
            "#c.not",
            None,
            "logical not",
            vec![("x", Type::Bool)],
            Type::Bool,
        );

        self.core(
            "#c.print",
            Some("print"),
            "print any value to stdout and return it",
            vec![("x", Type::Any)],
            Type::Any,
        );
        self.core(
            "#c.assert_eq",
            None,
            "assert equal",
            vec![("a", Type::Any), ("b", Type::Any)],
            Type::Bool,
        );
    }

    pub fn resolve_target(&self, target: &str) -> Option<String> {
        if target.starts_with('#') {
            if self.buckets.contains_key(target) {
                Some(target.to_string())
            } else {
                None
            }
        } else {
            self.label_to_id.get(target).cloned()
        }
    }

    pub fn get(&self, id: &str) -> Option<&Bucket> {
        self.buckets.get(id)
    }
}
