use crate::ast::Type;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
}

impl Value {
    pub fn ty(&self) -> Type {
        match self {
            Value::Num(_) => Type::Num,
            Value::Bool(_) => Type::Bool,
            Value::Str(_) => Type::Str,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Value::Num(n) => format_num(*n),
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
        }
    }

    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => (a - b).abs() < 1e-9,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
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
