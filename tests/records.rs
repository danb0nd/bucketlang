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
    let src = std::fs::read_to_string("examples/records.bkt").unwrap();
    let compiled = compile(&src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Num(1.0));
}

#[test]
fn list_of_records() {
    let src = r#"
    @test first_x([{ x: 1, y: 2 }, { x: 3, y: 4 }]) == 1
    first_x(ps: List[{ x: Num, y: Num }]) -> Num "x of first point" {
      if list_len(ps) == 0 then 0 else list_nth(ps, 0).x
    }
    @entry
    main() -> Num "entry" {
      first_x([{ x: 5, y: 6 }])
    }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
}
