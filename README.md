# bucketlang

A small **toy expression language**. Programs are graphs of little typed functions
called **buckets**, each with an address, a label, an English description, a
contract, and a one-expression body.

It runs by **compile → in-memory IR → interpret**. It is not fast, it is not
production anything, and nothing depends on it. It exists because writing a
language is a good way to spend a weekend, and this one turned out nice enough to
keep around.

Repo: https://github.com/danb0nd/bucketlang

```text
triple(x: Num) -> Num "multiply by 3" {
  x * 3
}

@entry
main() -> Num "print triple(5)" {
  print(triple(5))
}
```

---

## Where it came from

The starting idea was about LLMs, not about languages.

Coding agents paste whole files into context, and most of a file is irrelevant to
any single edit. So: what if code were shaped so an agent could edit **one
function at a time** and you could *prove* it only touched that one? That needs
three things a normal language doesn't separate —

- an **address** (`#b00000001`) as identity, so a rename doesn't move the node,
- a **description** as the interface, so a neighbour can be summarised in one
  line instead of pasted in full,
- **small bodies**, enforced by a complexity budget, so a bucket fits in a
  glance.

The full write-up of that idea is in [`og_idea.md`](og_idea.md), kept as a
historical record — the implementation diverged from it in a few places.

That experiment lives elsewhere now, and it turned out most of what it was
chasing gets solved by prompt caching and diff-shaped edits in languages models
already know. What's left here is the language, which is the part that was fun.

A few pieces of the original idea survived into it and are still the most
interesting things in the repo: [the three gates](#the-three-gates), and
diagnostics that name a bucket and a line as structured fields rather than prose.

---

## Quick start

```bash
cargo build --release
alias bkt=./target/release/bkt

bkt check examples/combo.bkt
bkt run examples/combo.bkt --arg 5          # prints 12

bkt run examples/sum_list.bkt               # lists + recursion
bkt run examples/records.bkt                # records + variants
bkt run examples/math.bkt                   # ** pow, fact, gcd
bkt run examples/modules/app.bkt            # import util::…
```

---

## Language surface

| Piece | Role |
|---|---|
| Buckets | Named functions with `(args) -> Ret "desc" { body }` |
| `@entry` / `@test` | Program entry; shadow tests (`#t…`) |
| `Num` `Bool` `Str` | Scalars |
| `List[T]` | Immutable lists + `list_*` cores |
| `{ x: Num, y: Num }` | Records; field access `p.x`; **punning** `{ x, y }` |
| `type Name = …` | Type aliases |
| `None \| Some(Num)` | Tagged variants + `match` |
| `if` / recursion | Control + algorithms |
| `module` / `import` | Nested packages (`std::option`); `#mod::b…`; aliases |
| `type Option[T]` | Parametric aliases; real `None`/`Some` (no sentinel records) |
| `\|\>` | Pipe: `x \|\> f` → `f(x)` |
| `to_json` / `--json` | Host interop; variants as `{"tag","payload"}` |
| `@test_error` | Assert a call fails; structural diffs on `@test` fail |
| `**` `pow` `mod` `floor` `abs` | Math cores; more via buckets |

Full syntax: [`SPEC.md`](SPEC.md). What exists today: [`LANGUAGE.md`](LANGUAGE.md).

### Minimal program

```text
@test triple(3) == 9
triple(x: Num) -> Num "multiply by 3" {
  x * 3
}

@entry
main() -> Num "print triple(5)" {
  print(triple(5))
}
```

### Bootstrap a structure

New data structures are `type` aliases plus constructors and helpers, not new
language cores.

```text
type Point = { x: Num, y: Num }
type OptNum = None | Some(Num)

point(x: Num, y: Num) -> Point "ctor" {
  { x, y }                    // field punning
}

unwrap_or(o: OptNum, d: Num) -> Num "default if None" {
  match o {
    None => d,
    Some(v) => v
  }
}
```

**Rule of thumb:** add a **core** only when userland can't do it honestly (e.g.
real `pow`). Add a **type + buckets** for new ADTs.

### Modules

Each file can declare a module. Imports resolve `name.bkt` or `name/mod.bkt` next
to the importer, then `./stdlib`, then `.`. Addresses stay unique by prefixing.

```text
// util.bkt
module util
double(x: Num) -> Num "×2" { x * 2 }

// app.bkt
module app
import util
import util::double as dbl

@entry
main() -> Num "use util" {
  dbl(21)                   // import alias
  util::double(21)          // qualified path
  // machine: #util::b00000001(21)
}
```

| Inside module `util` | Linked / from outside |
|---|---|
| `#util::b00000001` | same |
| label `double` | `util::double` / alias (`dbl`) — not bare `double` |

No module decl → classic `#b00000001` (single-file programs).

---

## CLI

```bash
# validate
bkt check file.bkt
bkt check file.bkt --release          # strip @tests from the program

# run (dev: tests then entry; stdout = print only)
bkt run file.bkt --arg 5
bkt run file.bkt --arg 5 -v           # tests summary + => result

# inspect layers / graph
bkt inspect file.bkt --manifest --graph
bkt inspect file.bkt --graph-dot | dot -Tsvg -o g.svg

# replace one bucket's body
bkt edit file.bkt --bucket sum --body '…'           # dry-run + run related tests
bkt edit file.bkt --bucket sum --body '…' --write   # persist if all gates pass
```

Profiles: default **dev** includes `#t…` tests; **`--release`** strips them.

### The three gates

`bkt edit` writes nothing until a proposed body passes all three:

| Gate | Catches |
|---|---|
| **compile** | it doesn't parse or typecheck |
| **atomicity** | it changed a bucket other than the target |
| **behaviour** | the bucket's own `@test` cases no longer pass |

The atomicity gate is the non-obvious one, and it's the bit of the original idea
worth keeping. A body is untrusted text; if it closes its own brace it can
define, delete, or rewrite a neighbour while still compiling and still passing
the target's tests. It's checked by comparing every user bucket's content hash
before and after the splice — which works for a body edit because addresses are
minted in source order, and a body edit doesn't change that order.

See [`crates/bucketlang/tests/splice.rs`](crates/bucketlang/tests/splice.rs) for
the cases it was built against.

---

## Layout

```text
crates/
  bucketlang/   the language — lexer, parser, types, eval, registry, graph,
                plus the grammar-aware parts of an edit: splice-by-span,
                run-a-bucket's-tests, the atomicity oracle
  bkt/          the CLI
stdlib/std/     std::option, std::core
examples/       see below
```

---

## Examples

| File | Shows |
|---|---|
| [`examples/combo.bkt`](examples/combo.bkt) | Small graph, tests, locals |
| [`examples/sum_list.bkt`](examples/sum_list.bkt) | `List`, `if`, recursion |
| [`examples/records.bkt`](examples/records.bkt) | `type`, records, punning, variants + `match` |
| [`examples/math.bkt`](examples/math.bkt) | `**` / cores + bootstrapped `fact` `gcd` `sqrt` |
| [`examples/modules/`](examples/modules/) | `module` / `import` / `util::double` |
| [`examples/locals.bkt`](examples/locals.bkt) | Multiple `print`s |
| [`examples/types.bkt`](examples/types.bkt) | `Str` / `Bool` |
| [`examples/dans_first_bkt.bkt`](examples/dans_first_bkt.bkt) | Minimal first program |

`combo`, `locals`, and `types` take an entry argument — `bkt run … --arg 5`, or
`--arg dan` for `types`. The rest run bare.

---

## Known rough edges

It's a toy, and these are the sharp bits. Listed rather than fixed, so nobody
has to rediscover them.

- **`&&` and `||` don't short-circuit.** They lower to ordinary core calls, so
  both sides always evaluate. `x != 0 && 10 / x > 1` raises on `x = 0`.
- **`Num` is `f64`, and `==` has a 1e-9 absolute tolerance.** So `1e-10 == 0` is
  true. The same comparison backs `@test` assertions, where the fuzz is a
  feature, and program logic, where it isn't.
- **A `match` binder escapes its arm at eval time** but not at typecheck time,
  which is a soundness hole: a bucket declared `-> Str` can return a `Num`.
- **`from_json` treats any object with a string `tag` field as a variant**, and
  drops the sibling keys. `{ tag: "sale", amount: 5 }` round-trips to
  `{"tag":"sale"}`.
- **Addresses shift when buckets are added or removed.** They're minted
  sequentially in source order. Stable across a body edit, not across an insert
  or a delete.
- **The atomicity gate hashes the body only**, so a neighbour's description or
  contract can change without tripping it.
- **`bkt edit` on a bucket with no `@test` runs the whole test suite**, and will
  blame your edit for a failure that was already there.
- **Complexity budgets are hard errors, not warnings** — 32 AST nodes, depth 10,
  12 calls per bucket. Deliberate, but it bites.

---

## Note

Early prototype, vibecoded with [Cursor](https://cursor.com). Ideas intentional;
implementation still a sketch. Not maintained toward anything in particular —
poke at it, fork it, break it.
