# Bucketlang SPEC (v0.1)

Expression-oriented language of **addressable buckets** (virtual memory slots) with contracts, descriptions, emergent call graphs, and a `bkt` CLI.

## Slot model

| Axis | Role |
|---|---|
| Address `#b…` / `#t…` / `#c.*` | Stable identity |
| Label | Optional sugar → address |
| Desc | `"…"` after return type (strict: required) |
| Contract | `(params: Num) -> Num` |
| Body | Single expression |
| Edges | Derived from calls |

## Syntax sketch

```text
add_one(x: Num) -> Num "adds one to the input" {
  x + 1
}

@test combo(5) == 12
@entry
combo(x: Num) -> Num "add one, then double" {
  print(double(add_one(x)))
}
```

- Extensions: `.bkt` or `.bucket`
- Manual address: `#b0000000a label(...) -> Num "…" { … }`
- `--non-strict` / `--anon`: omit label and/or desc

## Address spaces

- `#c.*` — reserved cores (`add` `sub` `mul` `div` `print` `assert_eq`)
- `#b…` — user buckets (sequential or manual)
- `#t…` — shadow test buckets from `@test`

## Complexity budget (user + test bodies)

max nodes 16, depth 6, calls 4 (after resolving infix to `#c.*`).

## Types

| Type | Literals | Notes |
|---|---|---|
| `Num` | `3`, `2.5` | `+ - * /`, comparisons |
| `Bool` | `true`, `false` | `&& \|\| !`, comparisons |
| `Str` | `"hi"` | `+` concatenates; `==` / `!=` |

`print` accepts any of these. `@test` expected values can be Num, Bool, or Str.


A bucket body is a block:

```text
main(x: Num) -> Num "demo" {
  y = x + 1        // local binding
  print(y)         // side-effect statement
  y * 2            // final expression = return value
}
```

`print(n)` writes `n` to stdout and returns `n`.

`bkt run` default: **only** that stdout. Use `--show-result`, `--show-tests`, or `-v` for extras.


```bash
bkt check file.bkt
bkt run file.bkt --arg 5
bkt inspect file.bkt          # all layers
bkt inspect file.bkt --graph --ast --json
```

`print` writes to stdout; entry return printed as `=> value` unless `--quiet-result`.
