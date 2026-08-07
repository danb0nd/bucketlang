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

## Language snapshot

```text
@test combo(5) == 12
combo(x: Num) -> Num "add one, then double" {
  double(add_one(x))
}

@entry
main(x: Num) -> Num "run it" {
  y = combo(x)
  print(y)
  y
}
```

- Types: `Num`, `Bool`, `Str`
- Locals: `name = expr`
- `print(value)` writes to stdout (and returns the value)
- `@entry` marks the program entrypoint
- `@test` lowers to shadow test buckets
- Strict mode (default) requires labels + descriptions; `--non-strict` allows anon slots

See [SPEC.md](SPEC.md) for the living contract.

## Build & run

Requires a recent Rust toolchain (`cargo`).

```bash
cargo build --release
./target/release/bkt check examples/combo.bkt
./target/release/bkt run examples/combo.bkt --arg 5
```

By default, `bkt run` only shows what your program `print`s:

```text
12
```

Opt into more:

```bash
bkt run file.bkt --arg 5 -v              # test summary + => result
bkt run file.bkt --arg 5 --show-result
bkt run file.bkt --arg 5 --show-tests
```

Inspect compiler layers (tokens, AST, manifest, graph, …):

```bash
bkt inspect examples/combo.bkt
bkt inspect examples/combo.bkt --graph --manifest
bkt inspect examples/combo.bkt --json | jq .
```

Optional Graphviz render:

```bash
brew install graphviz
bkt inspect examples/combo.bkt --graph-dot | dot -Tsvg -o graph.svg
```

## Status

Prototype / research code. Expect breaking changes. Useful today for experimenting with bucket-shaped programs, call graphs, and small typed expression bodies.

## License

MIT (see `Cargo.toml`).
