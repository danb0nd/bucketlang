use bucketlang::compile::{compile, compile_file, compile_with_base, BuildProfile, CompileOptions};
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
    assert!(compiled.imports.iter().any(|i| i.module == "util" && i.item.is_none()));
    assert!(compiled
        .imports
        .iter()
        .any(|i| i.module == "util" && i.item.as_deref() == Some("double") && i.alias.as_deref() == Some("dbl")));
    assert!(compiled.registry.buckets.contains_key("#util::b00000001"));
    assert!(compiled.registry.label_to_id.contains_key("util::double"));
    assert_eq!(
        compiled.registry.label_to_id.get("dbl"),
        compiled.registry.label_to_id.get("util::double")
    );
    assert_eq!(
        compiled.registry.label_to_id.get("sq"),
        compiled.registry.label_to_id.get("util::square")
    );

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
fn import_item_alias_only() {
    let src = r#"
    module app
    import util::double as foofoo
    @entry
    main() -> Num "call alias" { foofoo(3) }
    "#;
    let compiled =
        compile_with_base(src, strict_dev(), Path::new("examples/modules/app.bkt")).unwrap();
    assert_eq!(
        compiled.registry.label_to_id.get("foofoo"),
        compiled.registry.label_to_id.get("util::double")
    );
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out).unwrap();
    assert_eq!(v, Value::Num(6.0));
}

#[test]
fn import_does_not_leak_bare_labels() {
    let compiled = compile_file(Path::new("examples/modules/app.bkt"), strict_dev()).unwrap();
    assert!(compiled.registry.label_to_id.contains_key("util::double"));
    assert!(compiled.registry.label_to_id.contains_key("dbl"));
    // bare name from util stays inside util; importer must use path or alias
    assert!(!compiled.registry.label_to_id.contains_key("double"));
    assert!(!compiled.registry.label_to_id.contains_key("square"));
}

#[test]
fn same_label_two_modules_with_aliases() {
    let dir = std::env::temp_dir().join("bucketlang_mod_clash");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.bkt"), "module a\nfoo() -> Num \"a\" { 1 }\n").unwrap();
    std::fs::write(dir.join("b.bkt"), "module b\nfoo() -> Num \"b\" { 2 }\n").unwrap();
    let app = dir.join("app.bkt");
    std::fs::write(
        &app,
        r#"
module app
import a::foo as af
import b::foo as bf
@entry
main() -> Num "sum both foos" { af() + bf() }
"#,
    )
    .unwrap();

    let compiled = compile_file(&app, strict_dev()).unwrap();
    assert!(!compiled.registry.label_to_id.contains_key("foo"));
    assert!(compiled.registry.label_to_id.contains_key("a::foo"));
    assert!(compiled.registry.label_to_id.contains_key("b::foo"));
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out = Vec::new();
    let v = eval_bucket(&compiled.registry, entry, &[], &mut out).unwrap();
    assert_eq!(v, Value::Num(3.0));
}

#[test]
fn whole_module_import_requires_qualified_or_alias() {
    let src = r#"
    module app
    import util
    @entry
    main() -> Num "bare should fail" { double(3) }
    "#;
    assert!(compile_with_base(src, strict_dev(), Path::new("examples/modules/app.bkt")).is_err());

    let src2 = r#"
    module app
    import util
    @entry
    main() -> Num "qualified ok" { util::double(3) }
    "#;
    let compiled =
        compile_with_base(src2, strict_dev(), Path::new("examples/modules/app.bkt")).unwrap();
    let entry = compiled.registry.entry.as_deref().unwrap();
    let mut out = Vec::new();
    assert_eq!(
        eval_bucket(&compiled.registry, entry, &[], &mut out).unwrap(),
        Value::Num(6.0)
    );
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
    assert!(compiled.registry.entry.as_deref().unwrap().starts_with("#demo::b"));
}
