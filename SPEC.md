# Bucketlang SPEC (v0.1)

Expression-oriented language of **addressable buckets** (virtual memory slots) with contracts, descriptions, emergent call graphs, and a `bkt` CLI.

## Slot model

| Axis | Role |
|---|---|
| Address `#b…` / `#t…` / `#c.*` | Stable identity (virtual memory handle) |
| Label | Optional sugar → address |
| Desc | `"…"` string right after the return type (strict: required) |
| Contract | `(name: Type, …) -> Type` |
| Body | Block: locals / side-effect stmts + final expression |
| Edges | Derived from resolved calls |

## Syntax sketch

```text
// File extension: .bkt or .bucket
// Line comments: // ...

@test add_one(1) == 2
@test greet() == "hi"
@test is_pos(3) == true

add_one(x: Num) -> Num "adds one to the input" {
  x + 1
}

greet() -> Str "a greeting" {
  "hi"
}

is_pos(x: Num) -> Bool "is x positive?" {
  x > 0
}

// Optional: pin the address yourself
#b0000000a double(x: Num) -> Num "doubles the input" {
  x * 2
}

@test combo(5) == 12
combo(x: Num) -> Num "add one, then double" {
  double(add_one(x))
}

@entry
main(name: Str) -> Str "entrypoint: locals, print, strings" {
  y = combo(5)                 // local binding
  msg = "hello, " + name       // Str + Str => concat
  print(y)                     // side-effect (value discarded)
  print(msg)
  print(is_pos(y) && true)     // Bool ops
  msg                          // final expression = return value
}
```

### Surface forms

| Form | Meaning |
|---|---|
| `label(…) -> T "desc" { … }` | Define a user bucket (compiler mints `#b…`) |
| `#bdeadbeef label(…) -> T "desc" { … }` | Define at an explicit `#b` address |
| `#bdeadbeef (…) -> T { … }` | Anon bucket (needs `--non-strict`) |
| `@entry` | Next bucket is the program entry |
| `@test call(…) == expected` | Shadow test bucket (`#t…`) |
| `name = expr` | Local binding |
| `print(expr)` | Write value to stdout; return it (as a stmt, value discarded) |
| `label(…)` / `#addr(…)` | Call by label or address |
| `// comment` | Line comment |

### Expressions (precedence low → high)

```text
||                  // Bool or
&&                  // Bool and
== != < <= > >=     // compare (Num relations; ==/!= also Str/Bool)
+ -                 // Num arithmetic; Str + Str => concat
* /                 // Num
!  -                // unary not / negate
primary             // literal, var, call, (expr)
```

Literals: `3`, `2.5`, `true`, `false`, `"string"`.

### Types

| Type | Literals | Notes |
|---|---|---|
| `Num` | `3`, `2.5` | `+ - * /`, numeric comparisons |
| `Bool` | `true`, `false` | `&& \|\| !`, `== !=` |
| `Str` | `"hi"` | `+` concatenates; `== !=` |

`print` accepts any of these. `@test` expected values can be Num, Bool, or Str.

### Address spaces

| Prefix | Who | Notes |
|---|---|---|
| `#c.*` | Language | Cores: `add` `sub` `mul` `div` `eq` `ne` `lt` `gt` `le` `ge` `and` `or` `not` `print` `assert_eq` |
| `#b…` | User / compiler | Sequential mint or manual |
| `#t…` | Compiler | From `@test` only |

Labels are sugar; after compile, bodies use addresses. `print` → `#c.print`.

### Modes

- **Strict (default):** every user bucket needs a valid label + non-empty `"desc"`.
- **`--non-strict` / `--anon`:** label and/or desc may be omitted.

Identifiers: `[A-Za-z_][A-Za-z0-9_]*`, max 64, ASCII. Reserved: `Num` `Bool` `Str` `print` `true` `false`. Prefix `_` to silence unused warnings.

### Complexity budget (user + test bodies)

After resolving infix to `#c.*`: max **32** AST nodes, depth **10**, **8** call sites per bucket. Cores exempt.

## Body rules

```text
name(x: Num) -> Num "demo" {
  y = x + 1        // local
  print(y)         // side-effect statement
  y * 2            // final expression = return value
}
```

A body is a sequence of:

1. zero or more `name = expr` bindings and/or side-effect expressions (e.g. `print(…)`), then  
2. a **final expression** (the return value).  

If the last line is `name = expr`, that expression’s value is the return value.

## CLI

```bash
bkt check file.bkt
bkt run file.bkt --arg 5
bkt run file.bkt --arg hello          # Str / Bool / Num parsed from entry contract
bkt inspect file.bkt                  # all layers
bkt inspect file.bkt --graph --ast --json
```

`bkt run` default: **only** `print` output. Opt in with `--show-result`, `--show-tests`, or `-v`.  
Unused params/locals warn on stderr (`--no-warn` to silence).
