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
fn sum_list_example() {
    let src = std::fs::read_to_string("examples/sum_list.bkt").unwrap();
    let compiled = compile(&src, strict_dev()).unwrap();
    let reg = &compiled.registry;
    let mut out = Vec::new();
    for tid in &reg.test_ids {
        eval_bucket(reg, tid, &[], &mut out).unwrap();
    }
    let entry = reg.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(reg, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Num(10.0));
    assert!(String::from_utf8(out2).unwrap().contains("10"));
}

#[test]
fn list_ops_and_if() {
    let src = r#"
    @test head([9, 8]) == 9
    head(xs: List[Num]) -> Num "first element" {
      if list_len(xs) > 0 then list_nth(xs, 0) else 0
    }

    @test grow([1]) == [1, 2]
    grow(xs: List[Num]) -> List[Num] "append 2" {
      list_append(xs, 2)
    }

    @entry
    main() -> Num "entry" {
      head([1, 2, 3])
    }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
}
