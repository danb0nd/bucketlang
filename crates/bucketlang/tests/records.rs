use bucketlang::compile::{compile, BuildProfile, CompileOptions};
use bucketlang::eval::eval_bucket;
use bucketlang::value::Value;

fn strict_dev() -> CompileOptions {
    CompileOptions {
        strict: true,
        profile: BuildProfile::Dev,
    }
}

#[test]
fn records_example() {
    let src = std::fs::read_to_string("../../examples/records.bkt").unwrap();
    let compiled = compile(&src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        let tb = compiled.registry.get(tid).unwrap();
        let result = eval_bucket(&compiled.registry, tid, &[], &mut out);
        if tb.expect_error {
            assert!(result.is_err(), "expected error for {tid}");
        } else {
            result.unwrap();
        }
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Num(1.0));
}

#[test]
fn list_of_records() {
    let src = r#"
    type Point = { x: Num, y: Num }
    @test first_x([{ x: 1, y: 2 }, { x: 3, y: 4 }]) == 1
    first_x(ps: List[Point]) -> Num "x of first point" {
      if list_len(ps) == 0 then 0 else list_nth(ps, 0).x
    }
    @entry
    main() -> Num "entry" {
      first_x([{ x: 5, y: 6 }])
    }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    assert!(compiled.type_aliases.contains_key("Point"));
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
}

#[test]
fn type_alias_cycle_errors() {
    let src = r#"
    type A = B
    type B = A
    @entry
    main() -> Num "x" { 1 }
    "#;
    assert!(compile(src, strict_dev()).is_err());
}

#[test]
fn field_punning_and_match() {
    let src = r#"
    type Pair = { a: Num, b: Num }
    type Opt = None | Some(Num)
    @test mk(1, 2) == { a: 1, b: 2 }
    mk(a: Num, b: Num) -> Pair "punning ctor" { { a, b } }
    @test get(Some(3)) == 3
    get(o: Opt) -> Num "unwrap-ish" {
      match o { None => 0, Some(v) => v }
    }
    @entry
    main() -> Num "e" { get(Some(1)) }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
}
