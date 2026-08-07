# bucketlang

An experimental language where programs are **graphs of addressable buckets** — small semantic slots with stable IDs, contracts, and plain-language descriptions — plus a CLI (`bkt`) to run and inspect them.

## Why

Most coding agents still chew on whole files. That’s wasteful: context is expensive, and most of a file is irrelevant to one edit.

Bucketlang tries a different substrate:

1. **Code lives in buckets** — each bucket is a slot with a fixed address (`#b00000001`), optional human label, a required description, a typed contract, and a small expression body.
2. **Identity ≠ name ≠ meaning** — the address is the pointer; the label is sugar; the description is for humans/LLMs/retrieval; the body is the payload.
3. **The program is a graph** — calls create edges. No separate wiring file. `bkt inspect --graph` shows who calls whom (including tests and core ops).
4. **Keep buckets small** — complexity limits stop “one giant function” from collapsing the graph.
5. **Deterministic tooling first** — parse, typecheck, eval, graph, and tests are normal compiler work. The long-term idea is that an LLM edits *one bucket* over a *minimal subgraph*, not a whole repo dump.

This repo is an early prototype of that language and runtime — not the full agent harness yet.

## Mental model

```text
┌─────────────────────────────────────────┐
│  #b00000003   label: combo              │
│  "add one, then double"                 │
│  (x: Num) -> Num                        │
├─────────────────────────────────────────┤
│  double(add_one(x))                     │
└─────────────────────────────────────────┘
         │              │
         ▼              ▼
    #b00000001     #b00000002
     add_one         double
```

Reserved cores live in `#c.*` (`#c.add`, `#c.print`, …). Shadow tests from `@test` become real `#t…` buckets so behaviour checks sit in the same address space.

## Quick start

Requires a recent Rust toolchain (`cargo`).

```bash
git clone https://github.com/danb0nd/bucketlang.git
cd bucketlang
cargo build --release

# put bkt on your PATH for convenience, or use the path below
alias bkt=./target/release/bkt

bkt check examples/combo.bkt
bkt run examples/combo.bkt --arg 5
```

Default `bkt run` output is **only** what your program `print`s:

```text
12
```

## Write a program

Create a file ending in `.bkt` or `.bucket`.

```text
// hello.bkt
@test triple(3) == 9
triple(x: Num) -> Num "multiply by 3" {
  x * 3
}

@entry
main() -> Num "print triple(5)" {
  print(triple(5))
}
```

```bash
bkt run hello.bkt
# 15
```

### Anatomy of a bucket

```text
label(params) -> ReturnType "description for humans/LLMs" {
  body
}
```

| Piece | Meaning |
|---|---|
| `label` | Human sugar; compiles to a stable `#b…` address |
| `(x: Num)` | Typed params (contract) |
| `-> Num` | Return type |
| `"…"` | Mandatory description in strict mode (default) |
| `{ … }` | Body: locals, `print`, final expression |

Mark exactly one bucket with `@entry` — that’s what `bkt run` executes.

### Locals and print

```text
@entry
demo(x: Num) -> Num "locals + print" {
  a = x + 1
  b = a * 2
  print(a)
  print(b)
  b
}
```

```bash
bkt run examples/locals.bkt --arg 5
# 6
# 12
```

- `name = expr` binds a local  
- bare `print(…)` runs for effect  
- the **last** expression is the return value  

### Types

| Type | Literals | Useful ops |
|---|---|---|
| `Num` | `3`, `2.5` | `+ - * /` `< <= > >=` |
| `Bool` | `true`, `false` | `&& \|\| !` `== !=` |
| `Str` | `"hi"` | `+` (concat), `== !=` |

```bash
bkt run examples/types.bkt --arg world
# hello, world
# true
```

`--arg` is parsed according to the entry param type (`Num` / `Bool` / `Str`).

### Tests

```text
@test combo(5) == 12
@test combo(0) == 2
combo(x: Num) -> Num "add one, then double" {
  double(add_one(x))
}
```

Each `@test` becomes a shadow `#t…` bucket. `bkt run` / `bkt check` execute them; failures abort the run. Expected values can be `Num`, `Bool`, or `Str`.

### Addresses vs labels

Call by label or raw address (same slot):

```text
add_one(x)          // sugar
#b00000001(x)       // machine view
print(x)            // label for #c.print
```

Pin a slot yourself:

```text
#b0000000a add_one(x: Num) -> Num "adds one" {
  x + 1
}
```

## CLI cookbook

```bash
# validate (parse, types, contracts, complexity, @entry)
bkt check path/to/file.bkt

# run (@tests then @entry). stdout = print only
bkt run path/to/file.bkt
bkt run path/to/file.bkt --arg 5
bkt run path/to/file.bkt --arg hello          # Str entry
bkt run path/to/file.bkt --arg true           # Bool entry
echo 5 | bkt run path/to/file.bkt --stdin-arg

# show extra run info
bkt run file.bkt --arg 5 -v                   # tests + => result
bkt run file.bkt --arg 5 --show-result
bkt run file.bkt --arg 5 --show-tests

# inspect layers (great for learning / debugging)
bkt inspect file.bkt                          # everything
bkt inspect file.bkt --manifest --graph
bkt inspect file.bkt --ast --labelled
bkt inspect file.bkt --bucket combo --ast
bkt inspect file.bkt --json | jq .entry

# Graphviz
brew install graphviz
bkt inspect file.bkt --graph-dot | dot -Tsvg -o graph.svg
```

Unused params/locals warn on stderr (`--no-warn` to silence). Prefix with `_` if intentional (`_unused`).

Scratch without descriptions: `bkt check file.bkt --non-strict` (alias `--anon`).

## Bundled examples

| File | What it shows |
|---|---|
| [`examples/combo.bkt`](examples/combo.bkt) | Small graph, tests, locals, print |
| [`examples/locals.bkt`](examples/locals.bkt) | Multiple prints from one entry |
| [`examples/types.bkt`](examples/types.bkt) | `Str` / `Bool`, string concat, typed `--arg` |
| [`examples/dans_first_bkt.bkt`](examples/dans_first_bkt.bkt) | Minimal `@test` + `@entry` |

Try:

```bash
bkt run examples/combo.bkt --arg 5
bkt inspect examples/combo.bkt --graph
bkt run examples/types.bkt --arg Ada
```

## Language cheatsheet

```text
@test name(args) == expected
@entry
name(x: Num, flag: Bool, s: Str) -> Str "what this slot does" {
  y = x + 1
  ok = flag && (y > 0)
  print(ok)
  "hello, " + s
}
```

Full grammar and rules: [SPEC.md](SPEC.md).

## Status

Prototype / research code. Expect breaking changes. Useful today for experimenting with bucket-shaped programs, call graphs, and small typed expression bodies.

## License

MIT (see `Cargo.toml`).
