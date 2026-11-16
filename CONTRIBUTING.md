# Contributing to tree-sitter-mssql

A tree-sitter grammar for Microsoft SQL Server T-SQL, with Rust and Node.js bindings.

## Before you start

1. Branch from the latest **`main`** into a short-lived topic branch; `main` is the only long-lived branch and must stay green. Nothing is released automatically.
2. Read [ARCHITECTURE.md](ARCHITECTURE.md), [docs/scope.md](docs/scope.md), and [docs/development.md](docs/development.md).

## Local setup

```sh
npm install --ignore-scripts
npx --yes --package=tree-sitter-cli@0.27.2 -- tree-sitter generate
cargo build
```

`--ignore-scripts` skips the package's own `node-gyp-build` install script, which needs a C++ build toolchain. Regenerate the parser and commit the diff after any `grammar.js` change; a committed parser that has drifted from the grammar is a CI failure. On a machine without MSVC, prefix `tree-sitter test` and `tree-sitter parse` with `CC=gcc CXX=g++`.

## Validation before a pull request

```sh
npx --yes --package=tree-sitter-cli@0.27.2 -- tree-sitter test
node scripts/generate-node-kinds.mjs
node scripts/check-registries.mjs
node scripts/parser-cost.mjs
npm run lint
npm run format:check
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --manifest-path crates/tree-check/Cargo.toml -- test/fixtures/*.sql
cargo clippy --all-targets --manifest-path crates/tree-check/Cargo.toml -- -D warnings
cargo test --manifest-path crates/tree-check/Cargo.toml
```

`test/corpus/` holds per-construct tree assertions and `test/fixtures/*.sql` are larger files that `crates/tree-check` checks through the tree-sitter API (root `has_error`, `ERROR`, `MISSING`, zero-width nodes, full coverage); [docs/development.md](docs/development.md) has the details. CI (`Verify`) runs all of this, plus the Node binding tests (`npm test`), secret scanning, and workflow linting.

## Pull requests

- Open pull requests against **`main`** and merge with a merge commit (no squash or rebase: the imported upstream history must stay intact).
- Use a Conventional Commit title (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, ...).
- Keep each pull request to one change; link the issue it resolves in the body.
- Regenerate and commit `src/parser.c` and `test/node-kinds.txt` alongside any `grammar.js` change; CI diffs both against a fresh regenerate and fails on drift.
- Add a corpus case, or extend a fixture, for any new construct.
- A node-kind addition, removal, or rename is part of the published API surface (see [docs/consuming.md](docs/consuming.md)); call it out in the description so it reaches `CHANGELOG.md`.

## Documentation

| Document | Covers |
| --- | --- |
| [docs/scope.md](docs/scope.md) | What the grammar covers and how it treats SQL Server versions |
| [docs/consuming.md](docs/consuming.md) | Symbol, ABI, query files, node-kind stability |
| [docs/development.md](docs/development.md) | Building, testing, linting, fuzzing |
| [docs/accepted-gaps.md](docs/accepted-gaps.md) | Known differences from SQL Server |

## Fixing a defect

1. **Issue.** Open one issue per defect with the `defect` template: the minimal T-SQL input, SQL Server's verdict under `SET PARSEONLY ON` with compatibility level and engine version, and the current tree or `ERROR` excerpt.
2. **Failing case.** Add a corpus case under `test/corpus/` or an oracle case that fails before the change. SQL Server's verdict decides what is valid; where an upstream test or [docs/accepted-gaps.md](docs/accepted-gaps.md) disagrees, the change names the case and the compatibility level.
3. **Grammar change.** Edit `grammar.js` or `grammar/**/*.js` (or `src/scanner.c`), never the generated files.
4. **Regenerate.** Run `tree-sitter generate` with CLI 0.27.2 and commit `src/parser.c`, `src/grammar.json`, `src/node-types.json` and `test/node-kinds.txt` with the grammar change.
5. **Gates.** Run the checks in "Validation before a pull request". Existing corpus cases and fixtures keep passing unless the change alters them on purpose and says why.

Errors that need context (conflicting hints, boolean versus scalar, names, permissions) are not grammar defects; label them `contextual` instead of bending the grammar.

Issue labels:

| Label | Meaning |
| --- | --- |
| `under-accept` | SQL Server accepts the input; the tree has `ERROR` or `MISSING` |
| `over-accept` | SQL Server rejects the input with a syntax error; the tree is clean |
| `contextual` | SQL Server rejects the input for a reason that needs context, outside the grammar |
| `oracle` | The SQL Server oracle runner, its cases or its verdict files |
| `upstream-sync` | Merging changes from `alfpricot/tree-sitter-mssql` |
| `breaking` | Removes or renames a node kind or field name |
