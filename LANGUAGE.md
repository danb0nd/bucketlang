# Language inventory (what we have)

Snapshot of **bucketlang** as of the modules work. This is the mental map of the language + tooling.

## One-sentence pitch

Programs are **graphs of small typed functions (“buckets”)** with stable addresses, inline descriptions/tests, interpreted for an LLM edit loop, with a path to AOT binaries later.

## Core model

| Idea | Reality today |
|---|---|
| Bucket | Function-like slot: label, `"desc"`, `(args) -> Ret`, body |
| Address | `#b…` / `#mod/b…` (user), `#t…` / `#mod/t…` (tests), `#c.*` (cores) |
| Label | Sugar for an address (`double` → `#b…`) |
| Graph | Edges from calls; `bkt inspect --graph` |
| Strict mode | Label + non-empty desc required (default) |
| Complexity | Per-bucket budgets (nodes / depth / calls) |

## Types

| Type | Notes |
|---|---|
| `Num` `Bool` `Str` | Scalars |
| `List[T]` | Immutable; `list_len` `list_nth` `list_append` `list_concat` `list_remove` |
| `{ x: Num, y: Num }` | Records; `p.x`; punning `{ x, y }` |
| `None \| Some(Num)` | Tagged variants; exhaustive `match` |
| `type Name = …` | Aliases (expand at compile) |

## Control & computation

- `if cond then a else b`
- Recursion / mutual calls (depth guard 256)
- Locals: `name = expr` in a block; last expr is return
- `print(x)` side-effect
- Ops: `+ - * / **`, compares, `&& \|\| !`
- Math cores: `pow` `mod` `floor` `abs` (rest bootstrapped as buckets)

## Spec & tests

- `@entry` — program entry for `bkt run`
- `@test call(…) == expected` — shadow `#t…` buckets; run in **dev**, stripped in **`--release`**

## Modules

```text
module util          // prefixes addresses #util/b…
import util          // loads util.bkt or util/mod.bkt
util::double(3)      // cross-module call
```

No `module` → single-file `#b…` addresses (fine for small examples).

## Bootstrap rule

- **New ADTs** → `type` + records/variants + constructor/helper buckets  
- **New cores** → only when userland can’t be honest (`pow`, list ops, …)

## Tooling (`bkt`)

| Command | Role |
|---|---|
| `check` | Parse, typecheck, budgets, require `@entry` |
| `run` | Dev: tests then entry; stdout = `print` only |
| `inspect` / `dump` | Tokens, AST, manifest, graph, JSON, … |
| `context` | JSON pack for one bucket (LLM) |
| `edit` | Replace one body, recompile, run related tests |

Runtime path: **source → Registry IR → interpret**. AOT binary is planned, not built.

## Examples

| Path | Focus |
|---|---|
| `examples/combo.bkt` | Small graph + tests |
| `examples/sum_list.bkt` | Lists + recursion |
| `examples/records.bkt` | Records, punning, variants |
| `examples/math.bkt` | `**` + bootstrapped helpers |
| `examples/modules/` | `module` / `import` |

## Not yet

- Agent driver / scratchpad loop (beyond `context`/`edit`)
- AOT `bkt build` → binary
- Heap/`Ptr`, row polymorphism
- Package registry (only file-relative imports)
- LSP / pretty errors with spans everywhere

## Design verdict (honest)

**Strong:** addressable buckets + graph + inline tests/descs is a coherent LLM substrate; records/variants/lists/modules are enough to build real small programs and grow a stdlib in userland.

**Watch:** variant tags are globally unique in a linked program; complexity budgets can bite recursive list code; modules are intentionally simple (no re-exports, no `pub`).
