use bucketlang::compile::{compile, CompileOptions};
use bucketlang::eval::eval_bucket;
use bucketlang::value::Value;

#[test]
fn combo_five_is_twelve() {
    let src = std::fs::read_to_string("examples/combo.bkt").unwrap();
    let compiled = compile(&src, CompileOptions { strict: true }).unwrap();
    let reg = &compiled.registry;
    assert!(reg.entry.is_some());
    let mut out = Vec::new();
    for tid in &reg.test_ids {
        eval_bucket(reg, tid, &[], &mut out).unwrap();
    }
    let entry = reg.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(reg, entry, &[Value::Num(5.0)], &mut out2).unwrap();
    assert_eq!(v, Value::Num(12.0));
    let printed = String::from_utf8(out2).unwrap();
    assert!(printed.contains("12"));

    let combo = reg.label_to_id.get("combo").unwrap();
    let mut out3 = Vec::new();
    let v2 = eval_bucket(reg, combo, &[Value::Num(5.0)], &mut out3).unwrap();
    assert_eq!(v2, Value::Num(12.0));
    assert!(out3.is_empty());
}

#[test]
fn bool_and_str_work() {
    let src = r#"
    @test greet() == "hi"
    greet() -> Str "hello" {
      "hi"
    }
    @test is_pos(3) == true
    is_pos(x: Num) -> Bool "positive?" {
      x > 0
    }
    @entry
    main() -> Str "entry" {
      print(greet())
      print(is_pos(2))
      "ok"
    }
    "#;
    let compiled = compile(src, CompileOptions { strict: true }).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Str("ok".into()));
    let printed = String::from_utf8(out2).unwrap();
    assert!(printed.contains("hi"));
    assert!(printed.contains("true"));
}

#[test]
fn warns_unused_param() {
    let src = r#"
    @entry
    main(a: Num) -> Num "entry" {
      print(1)
    }
    "#;
    let compiled = compile(src, CompileOptions { strict: true }).unwrap();
    let warns = bucketlang::unused_warnings(&compiled.registry);
    assert!(warns
        .iter()
        .any(|w| w.message.contains("unused parameter 'a'")));
}

#[test]
fn non_strict_allows_anon() {
    let src = r#"
    @entry
    #b00000001 (x: Num) -> Num {
      x + 1
    }
    "#;
    let compiled = compile(src, CompileOptions { strict: false }).unwrap();
    let mut out = Vec::new();
    let v = eval_bucket(&compiled.registry, "#b00000001", &[Value::Num(41.0)], &mut out)
        .unwrap();
    assert_eq!(v, Value::Num(42.0));
}
