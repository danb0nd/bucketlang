//! Diagnostics.
//!
//! An error here is aimed at two readers at once: a human skimming a terminal,
//! and an agent deciding what to edit next. Both need the same facts — which
//! bucket, which line, which sub-expression, what was expected, what was found —
//! so the diagnostic carries them as *fields* and renders to text or JSON,
//! rather than baking them into a sentence that has to be parsed back out.
//!
//! The `bucket` field is the load-bearing one: it names the unit to fix, which
//! is what lets a caller scope a fix to one bucket instead of the whole file.

use crate::ast::Span;
use serde::Serialize;

/// Which compiler stage produced the diagnostic. Lets a caller filter, and tells
/// an agent whether the problem is in the text it wrote, the names it used, the
/// types it assumed, or what happened at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Parse,
    Link,
    Resolve,
    Type,
    Complexity,
    Eval,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Parse => "parse",
            Stage::Link => "link",
            Stage::Resolve => "resolve",
            Stage::Type => "type",
            Stage::Complexity => "complexity",
            Stage::Eval => "eval",
        }
    }
}

/// Where a diagnostic points. Byte offsets come from the AST; line/column are
/// resolved lazily against the source, because most errors are never rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Location {
    pub start: usize,
    pub end: usize,
}

impl From<Span> for Location {
    fn from(s: Span) -> Self {
        Location {
            start: s.start,
            end: s.end,
        }
    }
}

/// A single problem, with everything needed to act on it.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub stage: Stage,
    /// Stable machine-readable identifier, e.g. `type_mismatch`. Safe to match
    /// on; the prose in `message` is not.
    pub code: &'static str,
    pub message: String,
    /// Address of the bucket at fault — the unit an agent should re-edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    /// Human label for that bucket, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Line/column, filled in by `with_source` when a source text is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<usize>,
    /// The exact source text the location covers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found: Option<String>,
    /// What to do about it. Present whenever there is a concrete next step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl Diagnostic {
    pub fn new(stage: Stage, code: &'static str, message: impl Into<String>) -> Self {
        Diagnostic {
            stage,
            code,
            message: message.into(),
            bucket: None,
            bucket_label: None,
            location: None,
            line: None,
            col: None,
            snippet: None,
            expected: None,
            found: None,
            hint: None,
            notes: Vec::new(),
            file: None,
        }
    }

    pub fn at(mut self, span: Option<Span>) -> Self {
        self.location = span.map(Location::from);
        self
    }

    pub fn in_bucket(mut self, addr: impl Into<String>) -> Self {
        self.bucket = Some(addr.into());
        self
    }

    pub fn with_label(mut self, label: Option<String>) -> Self {
        self.bucket_label = label;
        self
    }

    pub fn expected(mut self, e: impl Into<String>) -> Self {
        self.expected = Some(e.into());
        self
    }

    pub fn found(mut self, f: impl Into<String>) -> Self {
        self.found = Some(f.into());
        self
    }

    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }

    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.notes.push(n.into());
        self
    }

    /// Resolve byte offsets into line/column and pull the offending text out of
    /// the source. Call once, at the boundary where a source string is known.
    pub fn with_source(mut self, source: &str, file: Option<&str>) -> Self {
        self.file = file.map(|f| f.to_string());
        if let Some(loc) = self.location {
            if loc.start <= source.len() && source.is_char_boundary(loc.start) {
                let (line, col) = line_col(source, loc.start);
                self.line = Some(line);
                self.col = Some(col);
                let end = loc.end.min(source.len());
                if end > loc.start && source.is_char_boundary(end) {
                    self.snippet = Some(source[loc.start..end].to_string());
                }
            }
        }
        self
    }

    /// Terminal rendering: a header line, then the source line with the offending
    /// span underlined, then the hint.
    pub fn render(&self, source: Option<&str>) -> String {
        let mut out = String::new();
        out.push_str(&format!("error[{}]: {}", self.code, self.message));

        let where_ = match (&self.file, self.line, self.col) {
            (Some(f), Some(l), Some(c)) => Some(format!("{f}:{l}:{c}")),
            (None, Some(l), Some(c)) => Some(format!("{l}:{c}")),
            _ => None,
        };
        let who = match (&self.bucket, &self.bucket_label) {
            (Some(a), Some(l)) => Some(format!("in bucket {l} ({a})")),
            (Some(a), None) => Some(format!("in bucket {a}")),
            _ => None,
        };
        match (where_, who) {
            (Some(w), Some(b)) => out.push_str(&format!("\n  --> {w}  {b}")),
            (Some(w), None) => out.push_str(&format!("\n  --> {w}")),
            (None, Some(b)) => out.push_str(&format!("\n  --> {b}")),
            (None, None) => {}
        }

        if let (Some(src), Some(line), Some(col), Some(loc)) =
            (source, self.line, self.col, self.location)
        {
            if let Some(text) = src.lines().nth(line.saturating_sub(1)) {
                let gutter = line.to_string();
                let pad = " ".repeat(gutter.len());
                // Width in characters, so the caret lines up with the text above.
                let width = src
                    .get(loc.start..loc.end.min(src.len()))
                    .map(|s| s.chars().count().max(1))
                    .unwrap_or(1);
                let caret_pad = " ".repeat(col.saturating_sub(1));
                out.push_str(&format!("\n{pad} |"));
                out.push_str(&format!("\n{gutter} | {text}"));
                out.push_str(&format!(
                    "\n{pad} | {caret_pad}{}",
                    "^".repeat(width.min(120))
                ));
                if let (Some(e), Some(f)) = (&self.expected, &self.found) {
                    out.push_str(&format!(" expected {e}, found {f}"));
                }
                out.push_str(&format!("\n{pad} |"));
            }
        } else if let (Some(e), Some(f)) = (&self.expected, &self.found) {
            out.push_str(&format!("\n  expected {e}, found {f}"));
        }

        for n in &self.notes {
            out.push_str(&format!("\n  = note: {n}"));
        }
        if let Some(h) = &self.hint {
            out.push_str(&format!("\n  = hint: {h}"));
        }
        out
    }
}

/// 1-based line and column (column counted in characters) for a byte offset.
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut last_break = 0;
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            last_break = i + 1;
        }
    }
    let col = source
        .get(last_break..offset)
        .map(|s| s.chars().count() + 1)
        .unwrap_or(1);
    (line, col)
}

#[derive(Debug, Clone)]
pub struct Error(pub Box<Diagnostic>);

impl Error {
    /// A message with no location. Kept because plenty of failures genuinely have
    /// no source position — a missing import, a bad CLI argument.
    pub fn msg(m: impl Into<String>) -> Self {
        Error(Box::new(Diagnostic::new(Stage::Link, "error", m)))
    }

    /// Parse-stage error at a known line/column.
    pub fn at(kind: &'static str, line: usize, col: usize, message: impl Into<String>) -> Self {
        let stage = match kind {
            "parse" => Stage::Parse,
            "type" => Stage::Type,
            "eval" => Stage::Eval,
            _ => Stage::Link,
        };
        let mut d = Diagnostic::new(stage, kind, message);
        d.line = Some(line);
        d.col = Some(col);
        Error(Box::new(d))
    }

    pub fn diag(d: Diagnostic) -> Self {
        Error(Box::new(d))
    }

    pub fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }

    pub fn into_diagnostic(self) -> Diagnostic {
        *self.0
    }

    /// Attach the bucket being compiled or evaluated, unless one is already set.
    /// The innermost frame wins, which is the one an agent should edit.
    pub fn in_bucket(mut self, addr: &str, label: Option<&str>) -> Self {
        if self.0.bucket.is_none() {
            self.0.bucket = Some(addr.to_string());
            self.0.bucket_label = label.map(|s| s.to_string());
        }
        self
    }

    /// Fill in whatever this diagnostic is still missing as it unwinds.
    ///
    /// Recursive checkers and evaluators call this on the way out, so a bare
    /// message raised deep inside picks up the span of the smallest expression
    /// that contains it and the address of the bucket it happened in. Fields
    /// already set are never overwritten — the innermost frame is the precise
    /// one, and it gets there first.
    pub fn fill(mut self, stage: Stage, span: Option<Span>, bucket: &str) -> Self {
        if self.0.location.is_none() {
            self.0.location = span.map(Location::from);
        }
        if self.0.bucket.is_none() {
            self.0.bucket = Some(bucket.to_string());
        }
        let _ = &self.0.bucket_label;
        // `msg` defaults to Link; once we know the stage, record it.
        if self.0.stage == Stage::Link {
            self.0.stage = stage;
            if self.0.code == "error" {
                self.0.code = match stage {
                    Stage::Type => "type_error",
                    Stage::Eval => "eval_error",
                    Stage::Resolve => "resolve_error",
                    _ => "error",
                };
            }
        }
        self
    }

    /// Fill in line/column and snippet from the source this came from.
    pub fn with_source(self, source: &str, file: Option<&str>) -> Self {
        Error(Box::new(self.0.with_source(source, file)))
    }

    /// Record the human label of the bucket at fault, if it has one and none is set.
    pub fn with_bucket_label(mut self, label: Option<String>) -> Self {
        if self.0.bucket_label.is_none() {
            self.0.bucket_label = label;
        }
        self
    }

    /// Attach a suggested next step, unless one is already present. Set by the
    /// site that knows the fix; deeper frames keep their more specific advice.
    pub fn or_hint(mut self, h: impl Into<String>) -> Self {
        if self.0.hint.is_none() {
            self.0.hint = Some(h.into());
        }
        self
    }

    pub fn render(&self, source: Option<&str>) -> String {
        self.0.render(source)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // One line, for contexts that just interpolate the error.
        let d = &self.0;
        write!(f, "{}", d.message)?;
        if let (Some(line), Some(col)) = (d.line, d.col) {
            write!(f, " at {line}:{col}")?;
        }
        if let Some(b) = &d.bucket {
            write!(f, " (in {b})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
