//! The feedback half of the system.
//!
//! An agent editing one bucket at a time can only stay surgical if the error
//! tells it *which bucket* and *which sub-expression* to change. These tests pin
//! that contract: every diagnostic must carry a machine-readable code, the
//! address of the bucket at fault, and a span narrow enough to be useful.

use bucketlang::compile::{compile, BuildProfile, CompileOptions};
use bucketlang::error::{Error, Stage};
use bucketlang::eval::eval_bucket;

fn strict_dev() -> CompileOptions {
    CompileOptions {
        strict: true,
        profile: BuildProfile::Dev,
    }
}

fn compile_err(src: &str) -> Error {
    compile(src, strict_dev()).err().expect("expected a failure")
}

#[test]
fn type_error_names_bucket_span_and_expectation() {
    let src = "\
@entry
main() -> Num \"entry\" {
  add_one(true)
}

add_one(x: Num) -> Num \"plus one\" {
  x + 1
}
";
    let e = compile_err(src).with_source(src, Some("t.bkt"));
    let d = e.diagnostic();

    assert_eq!(d.stage, Stage::Type);
    assert_eq!(d.bucket.as_deref(), Some("#b00000001"), "wrong bucket");
    assert_eq!(d.bucket_label.as_deref(), Some("main"), "label missing");
    assert_eq!(d.line, Some(3), "wrong line");

    // The span must point at the offending argument, not the whole body.
    let snippet = d.snippet.as_deref().unwrap_or("");
    assert!(
        snippet.contains("true"),
        "span should cover the bad argument, got {snippet:?}"
    );
    assert!(
        !snippet.contains("add_one"),
        "span too wide, got {snippet:?}"
    );
}

#[test]
fn add_operand_mismatch_is_its_own_code_with_a_hint() {
    let src = "\
f(n: Num) -> Str \"bad concat\" {
  \"count: \" + n
}

@entry
main() -> Str \"go\" {
  f(3)
}
";
    let e = compile_err(src).with_source(src, Some("t.bkt"));
    let d = e.diagnostic();

    assert_eq!(d.code, "add_operand_mismatch");
    assert_eq!(d.bucket_label.as_deref(), Some("f"));
    assert_eq!(d.expected.as_deref(), Some("Num + Num or Str + Str"));
    assert_eq!(d.found.as_deref(), Some("Str + Num"));
    assert!(d.hint.is_some(), "this one should suggest a fix");
}

#[test]
fn string_concatenation_typechecks() {
    // `+` on two Str values concatenates. The evaluator and the combined type
    // rule always supported it; the core's parameter types were what refused.
    let src = "\
@entry
main() -> Str \"greet\" {
  \"hello, \" + \"world\"
}
";
    let c = compile(src, strict_dev()).expect("Str + Str should typecheck");
    let entry = c.registry.entry.clone().expect("entry");
    let mut out = Vec::new();
    let v = eval_bucket(&c.registry, &entry, &[], &mut out).unwrap();
    assert_eq!(v.to_json_string(), "\"hello, world\"");
}

#[test]
fn unknown_name_points_at_the_name() {
    let src = "\
@entry
main() -> Num \"entry\" {
  x + 1
}
";
    let e = compile_err(src).with_source(src, Some("t.bkt"));
    let d = e.diagnostic();
    assert_eq!(d.line, Some(3));
    assert!(
        d.message.contains('x'),
        "should name the unknown binding: {}",
        d.message
    );
    assert_eq!(d.bucket_label.as_deref(), Some("main"));
}

#[test]
fn parse_error_keeps_its_line() {
    let src = "\
@entry
main() -> Num \"entry\" {
  1 +
}
";
    let d = compile_err(src).into_diagnostic();
    assert_eq!(d.stage, Stage::Parse);
    assert!(d.line.is_some(), "parse errors must carry a line");
}

#[test]
fn runtime_error_points_into_the_body() {
    // Division by zero is only discoverable at run time, so the span has to come
    // from the resolved body — proving spans survive name resolution.
    let src = "\
@entry
main() -> Num \"entry\" {
  bad(10)
}

bad(x: Num) -> Num \"divides by zero\" {
  x / 0
}
";
    let c = compile(src, strict_dev()).unwrap();
    let entry = c.registry.entry.clone().unwrap();
    let mut out = Vec::new();
    let e = eval_bucket(&c.registry, &entry, &[], &mut out)
        .err()
        .expect("expected a runtime failure")
        .with_source(src, Some("t.bkt"));
    let d = e.diagnostic();

    assert_eq!(d.stage, Stage::Eval);
    assert_eq!(
        d.bucket_label.as_deref(),
        Some("bad"),
        "should blame the bucket that divided, not the caller"
    );
    assert_eq!(d.line, Some(7), "should point at the division");
}

#[test]
fn rendering_includes_source_line_and_caret() {
    let src = "\
f(n: Num) -> Str \"bad\" {
  \"x\" + n
}

@entry
main() -> Str \"go\" {
  f(1)
}
";
    let text = compile_err(src)
        .with_source(src, Some("t.bkt"))
        .render(Some(src));

    assert!(text.starts_with("error["), "needs a code header: {text}");
    assert!(text.contains("t.bkt:2:3"), "needs file:line:col: {text}");
    assert!(text.contains("in bucket f"), "needs the bucket: {text}");
    assert!(text.contains("\"x\" + n"), "needs the source line: {text}");
    assert!(text.contains('^'), "needs a caret: {text}");
    assert!(text.contains("hint:"), "needs the hint: {text}");
}

#[test]
fn diagnostics_serialize_for_the_agent_loop() {
    let src = "\
f(n: Num) -> Str \"bad\" {
  \"x\" + n
}

@entry
main() -> Str \"go\" {
  f(1)
}
";
    let d = compile_err(src).with_source(src, Some("t.bkt")).into_diagnostic();
    let j: serde_json::Value = serde_json::to_value(&d).unwrap();

    // The fields an agent routes on must all be present.
    for key in ["stage", "code", "message", "bucket", "line", "col"] {
        assert!(!j[key].is_null(), "missing {key} in {j}");
    }
    assert_eq!(j["bucket"], "#b00000001");
    assert_eq!(j["bucket_label"], "f");
}
