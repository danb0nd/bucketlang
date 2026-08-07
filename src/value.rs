use crate::ast::Type;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
    List(Vec<Value>),
    Record(BTreeMap<String, Value>),
    Variant {
        tag: String,
        payload: Option<Box<Value>>,
    },
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
            Value::Variant { tag, payload } => {
                let mut tags = BTreeMap::new();
                tags.insert(tag.clone(), payload.as_ref().map(|p| p.ty()));
                Type::Variant(tags)
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
            Value::Variant { tag, payload } => match payload {
                None => tag.clone(),
                Some(v) => format!("{tag}({})", v.display()),
            },
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
            (
                Value::Variant {
                    tag: t1,
                    payload: p1,
                },
                Value::Variant {
                    tag: t2,
                    payload: p2,
                },
            ) => {
                t1 == t2
                    && match (p1, p2) {
                        (None, None) => true,
                        (Some(a), Some(b)) => a.equals(b),
                        _ => false,
                    }
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

    /// Structural diff for LLM self-heal prompts.
    pub fn diff(&self, expected: &Value) -> String {
        let mut parts = Vec::new();
        Self::diff_into(expected, self, "", &mut parts);
        if parts.is_empty() {
            "values differ".into()
        } else {
            parts.join("; ")
        }
    }

    fn diff_into(expected: &Value, got: &Value, path: &str, out: &mut Vec<String>) {
        if expected.equals(got) {
            return;
        }
        match (expected, got) {
            (Value::Record(a), Value::Record(b)) => {
                for (k, ev) in a {
                    let p = format!("{path}.{k}");
                    match b.get(k) {
                        Some(gv) => Self::diff_into(ev, gv, &p, out),
                        None => out.push(format!("{p}: missing (expected {})", ev.display())),
                    }
                }
                for k in b.keys() {
                    if !a.contains_key(k) {
                        out.push(format!("{path}.{k}: unexpected {}", b[k].display()));
                    }
                }
            }
            (Value::List(a), Value::List(b)) => {
                if a.len() != b.len() {
                    out.push(format!(
                        "{path}: list len {} ≠ {}",
                        a.len(),
                        b.len()
                    ));
                }
                for (i, (ev, gv)) in a.iter().zip(b.iter()).enumerate() {
                    Self::diff_into(ev, gv, &format!("{path}[{i}]"), out);
                }
            }
            (
                Value::Variant {
                    tag: t1,
                    payload: p1,
                },
                Value::Variant {
                    tag: t2,
                    payload: p2,
                },
            ) => {
                if t1 != t2 {
                    out.push(format!("{path}: tag {t1} ≠ {t2}"));
                } else {
                    match (p1, p2) {
                        (Some(a), Some(b)) => {
                            Self::diff_into(a, b, &format!("{path}.{t1}"), out)
                        }
                        (None, None) => {}
                        _ => out.push(format!("{path}: payload mismatch for {t1}")),
                    }
                }
            }
            _ => {
                let loc = if path.is_empty() { "." } else { path };
                out.push(format!(
                    "{loc}: {} ≠ {}",
                    expected.display(),
                    got.display()
                ));
            }
        }
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Value::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    serde_json::json!(*n as i64)
                } else {
                    serde_json::json!(n)
                }
            }
            Value::Bool(b) => serde_json::json!(b),
            Value::Str(s) => serde_json::json!(s),
            Value::List(xs) => {
                serde_json::Value::Array(xs.iter().map(|v| v.to_json_value()).collect())
            }
            Value::Record(fields) => {
                let mut map = serde_json::Map::new();
                for (k, v) in fields {
                    map.insert(k.clone(), v.to_json_value());
                }
                serde_json::Value::Object(map)
            }
            Value::Variant { tag, payload } => {
                let mut map = serde_json::Map::new();
                map.insert("tag".into(), serde_json::json!(tag));
                match payload {
                    None => {}
                    Some(p) => {
                        map.insert("payload".into(), p.to_json_value());
                    }
                }
                serde_json::Value::Object(map)
            }
        }
    }

    pub fn to_json_string(&self) -> String {
        self.to_json_value().to_string()
    }

    pub fn from_json_value(v: &serde_json::Value) -> Result<Value, String> {
        match v {
            serde_json::Value::Null => Err("null JSON is not a bucketlang value".into()),
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(Value::Num)
                .ok_or_else(|| "invalid JSON number".into()),
            serde_json::Value::String(s) => Ok(Value::Str(s.clone())),
            serde_json::Value::Array(xs) => {
                let mut out = Vec::new();
                for x in xs {
                    out.push(Self::from_json_value(x)?);
                }
                Ok(Value::List(out))
            }
            serde_json::Value::Object(map) => {
                if let Some(tag) = map.get("tag").and_then(|t| t.as_str()) {
                    let payload = match map.get("payload") {
                        None => None,
                        Some(p) => Some(Box::new(Self::from_json_value(p)?)),
                    };
                    return Ok(Value::Variant {
                        tag: tag.to_string(),
                        payload,
                    });
                }
                let mut fields = BTreeMap::new();
                for (k, v) in map {
                    fields.insert(k.clone(), Self::from_json_value(v)?);
                }
                Ok(Value::Record(fields))
            }
        }
    }

    pub fn from_json_str(s: &str) -> Result<Value, String> {
        let v: serde_json::Value =
            serde_json::from_str(s).map_err(|e| format!("invalid JSON: {e}"))?;
        Self::from_json_value(&v)
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
            Type::List(_) | Type::Record(_) | Type::Variant(_) => Err(
                "List/Record/Variant CLI args not supported yet; build them in the program".into(),
            ),
            Type::Name(n) => Err(format!("unresolved type alias {n} in CLI arg")),
            Type::Param(n) => Err(format!("unresolved type param {n} in CLI arg")),
            Type::App { name, .. } => Err(format!("unexpanded type {name}[…] in CLI arg")),
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
