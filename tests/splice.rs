//! Regression tests for `splice_bucket_body`.
//!
//! The original implementation scanned the source text for the bucket's label and
//! took the next `{`. That matched labels inside `desc` strings and at call sites,
//! so an edit could land on a completely different bucket — and when the wrong
//! bucket happened to typecheck, the edit reported success while silently
//! corrupting the file. Each test below pins one of those cases.

use bucketlang::compile::{compile, BuildProfile, CompileOptions};
use bucketlang::harness::{only_target_changed, splice_bucket_body, user_hashes};
use bucketlang::lexer::tokenize;
use bucketlang::registry::Registry;

fn strict_dev() -> CompileOptions {
    CompileOptions {
        strict: true,
        profile: BuildProfile::Dev,
    }
}

/// Compile `src`, then splice — mirroring what `bkt edit` does.
fn edit(src: &str, bucket: &str, body: &str) -> Result<String, String> {
    let compiled = compile(src, strict_dev()).map_err(|e| e.to_string())?;
    splice_bucket_body(src, &compiled.registry, bucket, body).map_err(|e| e.to_string())
}

/// Body text of `label` after recompiling, so assertions are about structure
/// rather than whitespace.
fn body_of(src: &str, label: &str) -> String {
    let compiled = compile(src, strict_dev()).unwrap();
    let addr = compiled.registry.resolve_target(label).unwrap();
    let b = compiled.registry.get(&addr).unwrap();
    bucketlang::render::render_labelled(&b.body, &compiled.registry)
}

#[test]
fn token_offsets_survive_multibyte_source() {
    // Byte offsets, not char indices: a desc with non-ASCII text must not shift
    // every span after it.
    let src = "f(x: Num) -> Num \"héllo ünicode\" {\n  x * 2\n}\n";
    for t in tokenize(src).unwrap() {
        if t.kind == bucketlang::lexer::TokenKind::Eof {
            continue;
        }
        assert!(
            src.is_char_boundary(t.start),
            "token {:?} start {} is not a char boundary",
            t.kind,
            t.start
        );
    }
    // And the splice still lands correctly in that file.
    let out = edit(src, "f", "x * 3").unwrap();
    assert_eq!(body_of(&out, "f"), "(x * 3)");
    assert!(out.contains("héllo ünicode"), "desc was damaged: {out}");
}

#[test]
fn label_mentioned_in_another_desc_is_not_a_definition() {
    // "double" appears inside main's desc string. The old scanner matched it and
    // spliced into main.
    let src = "\
@entry
main() -> Num \"uses the double helper\" {
  print(twice(21))
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}

double(y: Num) -> Num \"unrelated\" {
  y + 0
}
";
    let out = edit(src, "double", "y * 3").unwrap();
    assert_eq!(body_of(&out, "double"), "(y * 3)");
    assert_eq!(body_of(&out, "twice"), "(x * 2)");
    assert!(out.contains("uses the double helper"));
}

#[test]
fn call_site_before_definition_is_not_a_definition() {
    let src = "\
@entry
main() -> Num \"entry\" {
  print(twice(21))
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}
";
    let out = edit(src, "twice", "x * 3").unwrap();
    assert_eq!(body_of(&out, "twice"), "(x * 3)");
}

#[test]
fn intervening_bucket_is_not_spliced() {
    let src = "\
@entry
main() -> Num \"entry\" {
  print(twice(21))
}

helper(a: Num) -> Num \"in between\" {
  a + 1
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}
";
    let out = edit(src, "twice", "x * 3").unwrap();
    assert_eq!(body_of(&out, "twice"), "(x * 3)");
    assert_eq!(body_of(&out, "helper"), "(a + 1)");
}

#[test]
fn silent_corruption_case_is_fixed() {
    // The dangerous one: the intervening bucket shares the target's param name, so
    // the misplaced body typechecked and every gate passed while `helper` was
    // rewritten and `twice` left untouched.
    let src = "\
@entry
main() -> Num \"entry\" {
  print(twice(21))
}

helper(x: Num) -> Num \"same param name\" {
  x + 1
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}
";
    let out = edit(src, "twice", "x * 3").unwrap();
    assert_eq!(body_of(&out, "twice"), "(x * 3)", "target not edited");
    assert_eq!(
        body_of(&out, "helper"),
        "(x + 1)",
        "collateral damage to helper"
    );
}

#[test]
fn address_and_label_select_the_same_bucket() {
    let src = "\
@entry
main() -> Num \"entry\" {
  twice(21)
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}
";
    let by_label = edit(src, "twice", "x * 3").unwrap();
    let compiled = compile(src, strict_dev()).unwrap();
    let addr = compiled.registry.resolve_target("twice").unwrap();
    let by_addr = edit(src, &addr, "x * 3").unwrap();
    assert_eq!(by_label, by_addr);
}

#[test]
fn unknown_bucket_is_rejected() {
    let src = "\
@entry
main() -> Num \"entry\" {
  1
}
";
    let err = edit(src, "nope", "2").unwrap_err();
    assert!(err.contains("unknown bucket"), "unexpected error: {err}");
}

#[test]
fn core_bucket_is_rejected() {
    let src = "\
@entry
main() -> Num \"entry\" {
  print(1)
}
";
    let err = edit(src, "print", "1").unwrap_err();
    assert!(err.contains("core"), "unexpected error: {err}");
}

#[test]
fn test_bucket_is_rejected() {
    let src = "\
@test twice(2) == 4
twice(x: Num) -> Num \"times two\" {
  x * 2
}

@entry
main() -> Num \"entry\" {
  twice(1)
}
";
    let compiled = compile(src, strict_dev()).unwrap();
    let tid = compiled.registry.test_ids.first().unwrap().clone();
    let err = splice_bucket_body(src, &compiled.registry, &tid, "true")
        .unwrap_err()
        .to_string();
    assert!(err.contains("@test"), "unexpected error: {err}");
}

#[test]
fn brace_injection_is_caught_by_the_atomicity_oracle() {
    // A body is untrusted text. If it closes its own brace early it can define,
    // delete, or rewrite neighbouring buckets — the splice itself is structural,
    // so this is the gate that has to catch it.
    let src = "\
@entry
main() -> Num \"entry\" {
  twice(21)
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}
";
    let base = compile(src, strict_dev()).unwrap();
    let before = user_hashes(&base.registry);
    let target = base.registry.resolve_target("twice").unwrap();

    let injected = "x * 3\n}\n\nsneaky(z: Num) -> Num \"injected\" {\n  z * 99";
    let out = splice_bucket_body(src, &base.registry, "twice", injected).unwrap();

    // The spliced text still compiles — that is exactly why the gate is needed.
    let after = compile(&out, strict_dev()).unwrap();
    let err = only_target_changed(&before, &user_hashes(&after.registry), &target).unwrap_err();
    assert!(err.contains("not atomic"), "unexpected error: {err}");
    assert!(err.contains("appeared"), "should report the new bucket: {err}");
}

#[test]
fn a_clean_edit_passes_the_atomicity_oracle() {
    let src = "\
@entry
main() -> Num \"entry\" {
  twice(21)
}

twice(x: Num) -> Num \"times two\" {
  x * 2
}
";
    let base = compile(src, strict_dev()).unwrap();
    let before = user_hashes(&base.registry);
    let target = base.registry.resolve_target("twice").unwrap();

    let out = splice_bucket_body(src, &base.registry, "twice", "x * 3").unwrap();
    let after = compile(&out, strict_dev()).unwrap();
    only_target_changed(&before, &user_hashes(&after.registry), &target).unwrap();
}

#[test]
fn cores_and_tests_carry_no_span() {
    // The span is what authorises a splice, so anything not written in this file
    // must not have one.
    let src = "\
@test twice(2) == 4
twice(x: Num) -> Num \"times two\" {
  x * 2
}

@entry
main() -> Num \"entry\" {
  twice(1)
}
";
    let compiled = compile(src, strict_dev()).unwrap();
    for (addr, b) in &compiled.registry.buckets {
        match b.kind {
            bucketlang::ast::BucketKind::User => {
                assert!(b.body_span.is_some(), "{addr} should have a span")
            }
            _ => assert!(b.body_span.is_none(), "{addr} should not have a span"),
        }
    }
    let _ = Registry::with_cores();
}
