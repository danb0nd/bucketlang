use bucketlang::compile::{compile, compile_file, BuildProfile, CompileOptions};
use bucketlang::eval::eval_bucket;
use bucketlang::value::Value;
use std::path::Path;

fn strict_dev() -> CompileOptions {
    CompileOptions {
        strict: true,
        profile: BuildProfile::Dev,
    }
}

#[test]
fn nested_std_option_package() {
    let compiled = compile_file(Path::new("../../examples/packages/app.bkt"), strict_dev()).unwrap();
    assert!(compiled.registry.label_to_id.contains_key("std::option::unwrap_or"));
    assert!(compiled.registry.buckets.contains_key("#std::option::b00000001"));
    assert!(compiled.parametric_aliases.contains_key("Option"));

    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        let tb = compiled.registry.get(tid).unwrap();
        let r = eval_bucket(&compiled.registry, tid, &[], &mut out);
        if tb.expect_error {
            assert!(r.is_err());
        } else {
            r.unwrap();
        }
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Num(3.0));
}

#[test]
fn pipe_and_option_num_str() {
    let src = r#"
    type Option[T] = None | Some(T)
    @test u(None) == 1
    u(o: Option[Num]) -> Num "n" { match o { None => 1, Some(v) => v } }
    @test s(Some("a")) == "a"
    s(o: Option[Str]) -> Str "s" { match o { None => "", Some(v) => v } }
    @entry
    main() -> Num "pipe" { Some(2) |> u }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    assert_eq!(
        eval_bucket(&compiled.registry, entry, &[], &mut Vec::new()).unwrap(),
        Value::Num(2.0)
    );
}

#[test]
fn test_error_and_json() {
    let src = r#"
    @test_error boom()
    boom() -> Num "x" { error("nope") }
    @entry
    main() -> { a: Num } "json" { { a: 1 } }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        let tb = compiled.registry.get(tid).unwrap();
        assert!(tb.expect_error);
        assert!(eval_bucket(&compiled.registry, tid, &[], &mut out).is_err());
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut Vec::new()).unwrap();
    assert_eq!(v.to_json_string(), r#"{"a":1}"#);
}
