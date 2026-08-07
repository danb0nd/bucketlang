use crate::ast::Type;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
    List(Vec<Value>),
    Record(BTreeMap<String, Value>),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Num(_) => Type::Num,
            Value::Bool(_) => Type::Bool,
            Value::Str(_) => Type::Str,
            Value::List(xs) => {
                let elem = xs.first().map(|v| v.ty()).unwrap_or(Type::Any);
                Type::List(Box::new(elem))
            }
            Value::Record(fields) => {
                let mut tys = BTreeMap::new();
                for (k, v) in fields {
                    tys.insert(k.clone(), v.ty());
                }
                Type::Record(tys)
            }
        }
    }

    pub fn display(&self) -> String {
        match self {
            Value::Num(n) => format_num(*n),
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
            Value::List(xs) => {
                let inner: Vec<String> = xs
                    .iter()
                    .map(|v| match v {
                        Value::Str(s) => format!("{s:?}"),
                        other => other.display(),
                    })
                    .collect();
                format!("[{}]", inner.join(", "))
            }
            Value::Record(fields) => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| match v {
                        Value::Str(s) => format!("{k}: {s:?}"),
                        other => format!("{k}: {}", other.display()),
                    })
                    .collect();
                format!("{{ {} }}", inner.join(", "))
            }
        }
    }

    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => (a - b).abs() < 1e-9,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::List(a), Value::List(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Value::Record(a), Value::Record(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(k).is_some_and(|w| v.equals(w)))
            }
            _ => false,
        }
    }

    pub fn as_num(&self) -> Result<f64, String> {
        match self {
            Value::Num(n) => Ok(*n),
            _ => Err(format!("expected Num, got {}", self.ty().name())),
        }
    }

    pub fn as_bool(&self) -> Result<bool, String> {
        match self {
            Value::Bool(b) => Ok(*b),
            _ => Err(format!("expected Bool, got {}", self.ty().name())),
        }
    }

    pub fn as_str(&self) -> Result<&str, String> {
        match self {
            Value::Str(s) => Ok(s),
            _ => Err(format!("expected Str, got {}", self.ty().name())),
        }
    }

    pub fn as_list(&self) -> Result<&[Value], String> {
        match self {
            Value::List(xs) => Ok(xs),
            _ => Err(format!("expected List, got {}", self.ty().name())),
        }
    }

    pub fn as_record(&self) -> Result<&BTreeMap<String, Value>, String> {
        match self {
            Value::Record(f) => Ok(f),
            _ => Err(format!("expected Record, got {}", self.ty().name())),
        }
    }
}

pub fn format_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Parse a CLI argument into a Value, optionally guided by expected type.
pub fn parse_arg(raw: &str, expected: Option<&Type>) -> Result<Value, String> {
    if let Some(ty) = expected {
        return match ty {
            Type::Num => raw
                .parse::<f64>()
                .map(Value::Num)
                .map_err(|_| format!("expected Num arg, got {raw:?}")),
            Type::Bool => match raw {
                "true" | "True" | "1" => Ok(Value::Bool(true)),
                "false" | "False" | "0" => Ok(Value::Bool(false)),
                _ => Err(format!("expected Bool arg (true/false), got {raw:?}")),
            },
            Type::Str => Ok(Value::Str(raw.to_string())),
            Type::List(_) | Type::Record(_) => Err(
                "List/Record CLI args not supported yet; build them in the program".into(),
            ),
            Type::Any => parse_arg_auto(raw),
        };
    }
    parse_arg_auto(raw)
}

fn parse_arg_auto(raw: &str) -> Result<Value, String> {
    match raw {
        "true" => Ok(Value::Bool(true)),
        "false" => Ok(Value::Bool(false)),
        _ => {
            if let Ok(n) = raw.parse::<f64>() {
                Ok(Value::Num(n))
            } else {
                Ok(Value::Str(raw.to_string()))
            }
        }
    }
}
