# bucketlang

A small **expression language** designed so an LLM can edit **one function at a time** against a **call graph**, with contracts and tests sitting next to the code.

The runtime today is: **compile → in-memory IR → interpret**.  
Later: same IR can **AOT-compile to a binary** when a program is “done.”

Repo: https://github.com/danb0nd/bucketlang

---

## What we are building (the point)

Most coding agents paste whole files into context. That’s wasteful.

Bucketlang’s bet:

1. **Code is buckets** — addressable slots (`#b00000001`) with a label, a `"description"`, a typed contract, and a small body.
2. **The program is a graph** — calls are edges. Tools can show the neighborhood of one bucket.
3. **Keep buckets small** — complexity budgets discourage monster functions.
4. **Spec travels with code** — `@test` and descriptions are inline, so an agent doesn’t need a separate test harness dump.
5. **Bootstrap in userland** — new data structures are mostly `type` aliases + constructors + helpers, not endless new language cores.
6. **LLM loop on the interpreter** — `bkt context` / `bkt edit` → recompile → run tests fast. Ship later with a binary backend.

This repo is the **language + `bkt` CLI**. The full multi-step agent driver is next, not done yet.

```text
.bkt source ──► compile (Registry IR) ──► interpret (harness / dev)
                         │
                         └──► AOT binary (planned)
```

---

## Quick start

```bash
git clone https://github.com/danb0nd/bucketlang.git
cd bucketlang
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

## Language surface (current)

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

### Modules

Each file can declare a module. Imports resolve `name.bkt` or `name/mod.bkt` next to the importer. Addresses stay unique by prefixing:

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

**Rule of thumb:** add a **core** only when userland can’t do it honestly (e.g. real `pow`). Add a **type + buckets** for new ADTs.

---

## CLI (LLM-oriented)

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

# live edit loop
bkt context file.bkt --bucket sum     # JSON pack: contract, body, tests, neighbors
bkt edit file.bkt --bucket sum --body '…'           # dry-run + run related tests
bkt edit file.bkt --bucket sum --body '…' --write   # persist if OK
```

Profiles: default **dev** includes `#t…` tests; **`--release`** strips them (finished-product shape).

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

Full syntax: [`SPEC.md`](SPEC.md).  
**Language inventory (what exists today):** [`LANGUAGE.md`](LANGUAGE.md).

---

## Roadmap (short)

**Done enough for bootstrap + iterate:** buckets, graph, tests, lists, records, variants, math cores, modules/imports, `context`/`edit`.

**Next:** richer agent driver (scratchpad + edit loop), then AOT `bkt build` to a binary.

**Deferred:** row polymorphism, heap/`Ptr`, package registry beyond file imports.

---

## Note

Early prototype, vibecoded with [Cursor](https://cursor.com). Ideas intentional; implementation still a sketch. PRs and sharp edges welcome.
