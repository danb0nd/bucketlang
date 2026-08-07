# Language inventory (what we have)

Snapshot of **bucketlang** after nested packages, `Option[T]`, pipes, JSON host, and LLM test feedback.

## One-sentence pitch

Programs are **graphs of small typed functions (“buckets”)** with stable addresses, inline descriptions/tests, interpreted for an LLM edit loop, with a path to AOT binaries later.

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
| `context` / `edit` | LLM iterate loop |

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
`line`/`col`, `expected`/`found`, and a `hint` — as fields, so the edit loop
routes on them without parsing prose. `bucket` is the one that matters most: it
feeds straight back into `bkt context --bucket`, which is what keeps a fix
scoped to one bucket instead of a file rewrite.

## Not yet

- Generic bucket signatures `foo[T](…)`
- Compiler traits / method dispatch
- IR codec levels L1–L5 (only L0, plain source, is implemented)
- AOT `bkt build`, heap/`Ptr`, package registry
