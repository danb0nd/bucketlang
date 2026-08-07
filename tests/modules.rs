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
fn module_import_example() {
    let compiled = compile_file(Path::new("examples/modules/app.bkt"), strict_dev()).unwrap();
    assert_eq!(compiled.module.as_deref(), Some("app"));
    assert!(compiled.imports.iter().any(|i| i == "util"));
    assert!(compiled.registry.buckets.contains_key("#util/b00000001"));
    assert!(compiled.registry.label_to_id.contains_key("util::double"));

    let mut out = Vec::new();
    for tid in &compiled.registry.test_ids {
        eval_bucket(&compiled.registry, tid, &[], &mut out).unwrap();
    }
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out2 = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out2).unwrap();
    assert_eq!(v, Value::Num(6.0));
    assert!(String::from_utf8(out2).unwrap().contains("25"));
}

#[test]
fn import_without_path_errors() {
    let src = r#"
    import util
    @entry
    main() -> Num "x" { 1 }
    "#;
    assert!(compile(src, strict_dev()).is_err());
}

#[test]
fn module_mints_prefixed_addrs() {
    let src = r#"
    module demo
    @entry
    main() -> Num "x" { 1 }
    "#;
    let compiled = compile(src, strict_dev()).unwrap();
    assert!(compiled.registry.entry.as_deref().unwrap().starts_with("#demo/b"));
}
