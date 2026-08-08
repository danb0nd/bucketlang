# Language inventory (what exists today)

Snapshot of **bucketlang** after nested packages, `Option[T]`, pipes, and the JSON host.

## One-sentence pitch

A toy language where programs are **graphs of small typed functions (“buckets”)** with sequential addresses and inline descriptions and tests, compiled to an in-memory IR and interpreted.

## Core model

| Idea | Reality today |
|---|---|
| Bucket | Function-like slot: label, `"desc"`, `(args) -> Ret`, body |
| Address | `#b…` / `#mod::b…` / `#std::option::b…` (user), `#t…`, `#c.*` (cores) |
| Label | Sugar → address (`util::double`, import aliases) |
| Graph | Edges from calls; `bkt inspect --graph` |
| Strict mode | Label + non-empty desc required (default) |
| Complexity | Per-bucket budgets (nodes / depth / calls) |

## Types

| Type | Notes |
|---|---|
| `Num` `Bool` `Str` | Scalars |
| `List[T]` | Immutable; `list_*` cores |
| `{ x: Num, y: Num }` | Records; `p.x`; punning `{ x, y }` |
| `type Option[T] = None \| Some(T)` | Parametric aliases; `Option[Num]` and `Option[Str]` coexist |
| `type Result[T, E] = Ok(T) \| Err(E)` | Same pattern |
| `match` | Exhaustive on variants — prefer over sentinel `{ ok, val }` records |

## Control & computation

- `if` / `match` / recursion (depth 256)
- `|>` pipe: `x |> f` → `f(x)`; `x |> f(y)` → `f(x, y)`
- `print`, `error(msg)`, `to_json` / `from_json`
- Math cores: `pow` `mod` `floor` `abs`

## Spec & tests

- `@entry` — `bkt run`
- `@test call(…) == expected` — structural diffs on failure
- `@test_error call(…)` — passes iff the call errors
- Dev runs tests; `--release` strips them

## Modules & packages

```text
module std::option          // #std::option::b00000001
import std::option
import std::option::unwrap_or as unwrap_or
```

Resolve `a::b` → `a/b.bkt` or `a/b/mod.bkt` under importer dir, `./stdlib`, or `.`.
Bare labels do not leak across modules.

Stdlib lives under [`stdlib/std/…`](stdlib/std/) (`std::option`, `std::core` conventions).

## Host / JSON

Values map 1:1 to JSON (variants: `{"tag":"Some","payload":…}`).  
`to_json` / `from_json` cores; `bkt run --json` prints the entry return value as JSON.

## Trait-like convention (no compiler traits)

In `std::core`: name helpers like `to_string_point` — **convention for LLMs**, not method dispatch.

## Tooling (`bkt`)

| Command | Role |
|---|---|
| `check` / `run` | Typecheck; tests + entry (`--json` for host) |
| `inspect` | Tokens, AST, graph, … |
| `edit` | Replace one bucket's body behind the compile / atomicity / behaviour gates |

## Diagnostics

Every expression carries a source span, so errors name a bucket *and* a line:

```text
error[type_mismatch]: expected Num, found Str
  --> examples/types.bkt:14:9  in bucket main (#b00000003)
   |
14 |   msg = "hello, " + name
   |         ^^^^^^^^^
```

Each diagnostic carries `stage`, a stable `code`, `bucket` + `bucket_label`,
`line`/`col`, `expected`/`found`, and a `hint` — as fields rather than prose, so
a caller can route on them without regexing the message. `bucket` is the useful
one: it names the unit to fix, not just the byte offset.

## Not implemented

- Generic bucket signatures `foo[T](…)`
- Compiler traits / method dispatch
- Short-circuiting `&&` / `||` (both sides always evaluate)
- Integers — `Num` is `f64` throughout
- AOT `bkt build`, heap/`Ptr`, package registry
