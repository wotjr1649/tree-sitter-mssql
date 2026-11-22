# Development

How to build and test the grammar. [.github/CONTRIBUTING.md](../.github/CONTRIBUTING.md) lists the checks a pull request must pass.

## Building

```sh
npm install --ignore-scripts
npm rebuild tree-sitter-cli
npx tree-sitter generate --abi "$(cat ci/tree-sitter-abi.txt)" --js-runtime native
cargo build
```

`npm rebuild tree-sitter-cli` downloads the binary pinned in `package.json` (`devDependencies.tree-sitter-cli`),
and `npx tree-sitter` runs the installed copy. Verify checks the CLI pin against the lockfile, the `allowScripts`
entry and the installed binary version. It passes the ABI from `ci/tree-sitter-abi.txt` to `generate` and checks
the emitted ABI.

The generated parser is committed, so Cargo can build the parser and scanner through `cc` without the CLI.
After changing `grammar.js` or a module under `grammar/`, regenerate and commit changes to `src/parser.c`,
`src/grammar.json`, `src/node-types.json` and `src/tree_sitter/*.h`. Release version changes in
`tree-sitter.json` also require regeneration.

The external scanner lexes nested block comments and checks sqlcmd boundaries. Most commands require whitespace
before their first argument, and required arguments must start on the same line. `:EXIT(…)` and `:!!command`
can directly adjoin their arguments. Compile `src/scanner.c` alongside `src/parser.c`.

`generate` takes under a minute on this grammar. Run it with a time cap and read the exit code, and check that `src/parser.c`'s modification time moved before trusting a test run — a failed `generate` leaves the previous parser in place.

## Testing

```sh
npx tree-sitter test
cargo test
```

On Windows without MSVC, put MinGW on PATH and install the GNU Rust toolchain with
`rustup toolchain install stable-x86_64-pc-windows-gnu`. Use `cargo +stable-x86_64-pc-windows-gnu` for Cargo
commands. In a POSIX shell, select the C and C++ compilers for the CLI as follows.

```sh
CC=gcc CXX=g++ npx tree-sitter test
```

In PowerShell, set `$env:CC = 'gcc'` and `$env:CXX = 'g++'` before running `npx tree-sitter test`.

`test/corpus/` holds the per-construct tree assertions. `test/corpus/errors.txt` includes manually maintained
error-recovery expectations. `tree-sitter test --update` cannot record unexpected `ERROR` or `MISSING` nodes
automatically. Review and update intentional error-tree changes by hand.

Every SQL file in `test/fixtures/` must pass `crates/tree-check`. The checker prints one JSON line per file and
checks the root's `has_error` flag, `ERROR` and `MISSING` nodes, zero-width nodes on nonempty input, and whether
the root ends at the input length. It also applies the tree-local constraints from
[ADR-0003](decisions/ADR-0003-option-set-constraints.md) (`crates/tree-check/src/constraints.rs` and
`crates/tree-check/src/constraints/`). For example, each QUEUE activation setting can occur at most once. Any
violation fails the check and is listed under `constraint_violations`. The CLI parse summary can still exit
successfully for a tree with zero-width nodes and no explicit `ERROR` or `MISSING`.

```sh
cargo run --manifest-path crates/tree-check/Cargo.toml -- test/fixtures/*.sql
# exit 0: every tree is clean; 1: some tree has a problem; 2: a file could not be read or parsed (no verdict)
```

In PowerShell with the GNU Rust toolchain, expand the file list before passing it to Cargo.

```powershell
$fixtures = (Get-ChildItem test/fixtures -Filter '*.sql').FullName
cargo +stable-x86_64-pc-windows-gnu run --manifest-path crates/tree-check/Cargo.toml -- @fixtures
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

`npm run format` applies formatting fixes. `lint` runs against `grammar.js`, `grammar/`, `scripts/` and
`bindings/node/`. `format` and `format:check` cover `grammar.js`, `grammar/**/*.js`, `scripts/*.mjs` and
`bindings/node/*.js`.

## Performance and fuzzing

`node scripts/parser-cost.mjs` compares the generated parser with `test/parser-cost.json`: the size of `src/parser.c`, the state, symbol, token, field and production counts, and the field-map entries (all and inherited; inherited entries make field lookups costlier for consumers). Any difference fails `Verify`. After a grammar change, run it with `--update` in the same pull request. Growth above 5 % warns, and growth above 10 % is refused unless `--approve-budget` is passed and the pull request names the cells the growth adds (ADR-0002 §6).

Full Verify runs report fixture throughput (bytes/ms) and `src/parser.c` size. The separate runtime cost gate
(`node scripts/runtime-cost.mjs --tree-sitter node_modules/tree-sitter-cli/tree-sitter`) compares library size,
parse time and peak RSS where available with the milestone in `test/runtime-cost.json`. It fails if a measured
metric exceeds the milestone's limit, currently 1.5× the 2.1.1 baseline. A budget change needs a recorded
decision and a new milestone (ADR-0002 §6).

Use [Parse benchmark](../.github/workflows/parse-bench.yml) to compare Callgrind instruction counts across
Git refs on Linux. Run it from Actions with the default refs (`v2.1.1 v2.2.0 HEAD`) or list your own in
`bench_refs`, baseline first. The run reports changes without applying a budget by default. Specify a
nonnegative decimal percentage in `bench_max_increase_pct` to fail when any comparison exceeds that
increase for an input set and parse mode. Benchmark workflow and script changes also run on pull requests.

To run locally from the repository root on Linux, install GCC, Node.js, curl and Valgrind with its development
headers, and make the desired refs available to Git.

```sh
bash scripts/parse-bench.sh
BENCH_REFS='v2.2.0 HEAD' BENCH_MAX_INCREASE_PCT=5 bash scripts/parse-bench.sh
```

The job summary compares each ref with the first. The `parse-instructions` artifact includes the raw profiles
and tallies, `results.json`, the input hashes, resolved refs and build environment. Local runs save
the same files under `build/parse-bench/run.*/artifacts/`.
See the [benchmark method](benchmarks.md#ci-instruction-counts).

The [Fuzz parser workflow](../.github/workflows/fuzz.yml) runs on relevant pull requests, weekly and on manual
dispatch. It fuzzes the parser and scanner with `tree-sitter/fuzz-action` and the tree-check consumer APIs with
the Rust `tree_check` target. To run the local `parse` target on Linux or macOS, install nightly Rust, clang
and `cargo-fuzz` (`cargo install cargo-fuzz --locked`). Seed the corpus from the fixtures and run these commands
from the repository root.

```sh
mkdir -p fuzz/corpus/parse
cp test/fixtures/*.sql fuzz/corpus/parse/
CC=clang CXX=clang++ \
  CFLAGS=-fsanitize=address,fuzzer-no-link \
  CXXFLAGS=-fsanitize=address,fuzzer-no-link \
  cargo +nightly fuzz run parse -- -max_total_time=300 -timeout=10
```

`cargo-fuzz` sets the instrumentation flags for the Rust side itself; the `CC`/`CFLAGS` settings are needed so the `cc` crate also builds `src/parser.c` and `src/scanner.c` with libFuzzer coverage and AddressSanitizer. A crashing input is written to `fuzz/artifacts/parse/`.
