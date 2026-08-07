# bucketlang

Experimental **bucket** language: programs are graphs of addressable expression slots with contracts and descriptions. Tooling binary: **`bkt`**.

## Quick start

```bash
cargo build --release
./target/release/bkt check examples/combo.bkt
./target/release/bkt run examples/combo.bkt --arg 5
./target/release/bkt inspect examples/combo.bkt --graph --manifest
```

`bkt run` **only prints what your program `print`s**. Extra info is opt-in:

```bash
bkt run file.bkt --arg 5 -v              # tests summary + => result
bkt run file.bkt --arg 5 --show-result   # => N only
bkt run file.bkt --arg 5 --show-tests    # test summary on stderr
```

Expected default run output for combo:

```text
12
```

### Graphviz (optional)

`--graph-dot` prints DOT. To render an image you need Graphviz’s `dot`:

```bash
brew install graphviz
cargo run -- inspect examples/combo.bkt --graph-dot | dot -Tsvg -o graph.svg
open graph.svg
```

Without `dot` installed, the pipe closes and older builds panicked; current `bkt` exits quietly on broken pipe — install Graphviz to actually render.

## Learn more

See [SPEC.md](SPEC.md).
