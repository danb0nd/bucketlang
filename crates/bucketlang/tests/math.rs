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
fn math_example() {
    let src = std::fs::read_to_string("../../examples/math.bkt").unwrap();
    let compiled = compile(&src, strict_dev()).unwrap();
    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Num(6.0));
    let printed = String::from_utf8(out2).unwrap();
    assert!(printed.contains("1024"));
    assert!(printed.contains("120"));
}
