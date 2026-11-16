# Development

How to build and test the grammar. [CONTRIBUTING.md](../CONTRIBUTING.md) lists the checks a pull request must pass.

## Building

```sh
npx --yes --package=tree-sitter-cli@0.27.2 -- tree-sitter generate
cargo build
```

The generated parser (`src/parser.c`, `src/grammar.json`, `src/node-types.json`) is committed, so consumers can build the C parser through `cc` during `cargo build` without needing the tree-sitter CLI themselves. Regenerate and commit the diff after any `grammar.js` change.

The block comment is lexed by `src/scanner.c` rather than a regex, because T-SQL block comments nest and a regex cannot match balanced nesting. The scanner also checks what follows a sqlcmd command name: whitespace or the end of input, and for a command that takes an argument, that the argument is on the same line. Those checks look at characters without consuming them, which a regex token cannot do. `src/scanner.c` must be compiled alongside `src/parser.c`.

`generate` takes under a minute on this grammar. Run it with a time cap and read the exit code, and check that `src/parser.c`'s modification time moved before trusting a test run — a failed `generate` leaves the previous parser in place.

## Testing

```sh
npx --yes --package=tree-sitter-cli@0.27.2 -- tree-sitter test
cargo test
```

On a machine without MSVC, point the CLI at another C compiler, and run Cargo with the GNU toolchain (`rustup toolchain install stable-x86_64-pc-windows-gnu`, then `cargo +stable-x86_64-pc-windows-gnu …`), which links with MinGW `gcc`:

```sh
CC=gcc CXX=g++ npx --yes --package=tree-sitter-cli@0.27.2 -- tree-sitter test
```

`test/corpus/` holds the per-construct tree assertions. `test/corpus/errors.txt` deliberately pins error recovery on invalid input and is hand-written — `tree-sitter test --update` refuses to touch it. Every file under `test/fixtures/*.sql` must also parse cleanly. `crates/tree-check` judges each tree through the tree-sitter API — root `has_error`, `ERROR`, `MISSING` and zero-width nodes, and whether the root spans the whole input — and prints one JSON line per file. It also applies the option-set constraint layer of [ADR-0003](decisions/ADR-0003-option-set-constraints.md) (`crates/tree-check/src/constraints.rs`, with rule families in `crates/tree-check/src/constraints/`): rules such as "each QUEUE activation setting at most once", which SQL Server enforces while parsing and the grammar deliberately does not encode. Violations make a tree fail like an `ERROR` node, and are listed under `constraint_violations`. The CLI's parse summary is not enough: it can report success for a tree that contains `ERROR` or only zero-width nodes.

```sh
cargo run --manifest-path crates/tree-check/Cargo.toml -- test/fixtures/*.sql
# exit 0: every tree is clean; 1: some tree has a problem; 2: a file could not be read or parsed (no verdict)
```

Regenerate `test/node-kinds.txt` after any change to a node kind or field name:

```sh
node scripts/generate-node-kinds.mjs
```

Check that every `create_*`/`alter_*`/`drop_*` rule is registered in its statement's `choice()`:

```sh
node scripts/check-registries.mjs
```

## Lint and format

`oxlint` and `oxfmt` are devDependencies (configured in `.oxlintrc.json` and `.oxfmtrc.json`), so they run from this checkout's own `node_modules` rather than through a one-off `npx --package=...` call. Install them once, skipping the package's own `node-gyp-build` install script, which needs a C++ build toolchain that linting does not:

```sh
npm install --ignore-scripts
npm run lint
npm run format:check
```

`npm run format` applies fixes. `lint` runs against `grammar.js`, `grammar/`, `test/`, and `bindings/node/`; `format` and `format:check` cover `grammar.js`, `grammar/**/*.js`, `test/*.mjs`, `scripts/*.mjs`, and `bindings/node/*.js`. `src/**` is generated and excluded.

## Performance and fuzzing

`node scripts/parser-cost.mjs` compares the generated parser with `test/parser-cost.json`: the size of `src/parser.c`, the state, symbol, token, field and production counts, and the field-map entries (all and inherited; inherited entries make field lookups costlier for consumers). Any difference fails `Verify`. After a grammar change, run it with `--update` in the same pull request. Growth above 5 % warns, and growth above 10 % is refused unless `--approve-budget` is passed and the pull request names the cells the growth adds (ADR-0002 §6).

Every `Verify` run's job summary reports the fixture set's parse throughput (bytes/ms) and the current `src/parser.c` size, so a change's effect on either is visible without a separate benchmark step. The `cargo-fuzz` target in `fuzz/` fuzzes the parser and `src/scanner.c` against arbitrary input. It is run locally rather than in CI, on Linux or macOS with a nightly Rust toolchain, `clang`, and `cargo-fuzz` installed (`cargo install cargo-fuzz --locked`). Seed the corpus from the fixtures and run it from the repository root:

```sh
mkdir -p fuzz/corpus/parse
cp test/fixtures/*.sql fuzz/corpus/parse/
CC=clang CXX=clang++ \
  CFLAGS=-fsanitize=address,fuzzer-no-link \
  CXXFLAGS=-fsanitize=address,fuzzer-no-link \
  cargo +nightly fuzz run parse -- -max_total_time=300 -timeout=10
```

`cargo-fuzz` sets the instrumentation flags for the Rust side itself; the `CC`/`CFLAGS` settings are needed so the `cc` crate also builds `src/parser.c` and `src/scanner.c` with libFuzzer coverage and AddressSanitizer. A crashing input is written to `fuzz/artifacts/parse/`.
