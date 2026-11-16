# WASM constraint checker for hosts without CGO

The native `tree-check` CLI and the WASI Preview 1 reactor use the same grammar and constraint rules from one Rust
library. A Go consumer can call this module without porting the rules to Go and without CGO. The module contains the
grammar and the tree-sitter 0.27.2 runtime, so its input is SQL source bytes, not a tree from an external runtime.

## Pinned build and identity

Use a checkout with full Git history, Rust **1.100.0** with the `wasm32-wasip1` target, and **wasi-sdk 34.0** with its
archive SHA-256 verified. The SDK URL and checksum are in [`wasm-pins.json`](../crates/tree-check/wasm-pins.json). The
Linux SDK is the same pin as go-treesitter; Verify checks the checksum before extraction and caches it. The build
script does not download tools. On Windows, use the GNU Rust host toolchain.

```sh
export WASI_SDK_PATH=/absolute/path/to/wasi-sdk-34.0-x86_64-linux
export CARGO_TARGET_DIR=target
node scripts/tree-check-wasm-build.mjs
cargo build --locked --manifest-path crates/tree-check/Cargo.toml --bin tree-check
node scripts/tree-check-wasm-parity.mjs \
  --wasm target/wasm32-wasip1/release/tree_check.reactor.wasm \
  --tree-check target/debug/tree-check
```

C and Rust optimization is pinned to `-O3`/`opt-level=3`, fat LTO, one codegen unit, panic abort, symbol stripping, and
debug info and incremental compilation disabled. Local absolute paths of the sources and the cache are remapped too.
The script prints the module size, SHA-256, build time, and the actual imports and exports. The default recipe does not
use Binaryen. If `WASM_OPT` is set explicitly, only **version 133** is allowed, and it selects a separate Binaryen
`-O3` recipe. No other version is silently substituted. Do not assume identical hashes for builds with a different
recipe or host toolchain; rebuild from the same pinned inputs to check.

`tc_identity()` returns an owned result handle that holds this JSON:

```json
{"abi_version":2,"grammar_commit":"<40 hex>","rules_sha256":"<64 hex>"}
```

`grammar_commit` is the last commit that changed the grammar source, the generated parser or the scanner. The script
rejects uncommitted grammar changes and shallow clones. `rules_sha256` is the SHA-256 of a concatenation, in this
order, of these files under `crates/tree-check/src/`: `constraints.rs`, then
`constraints/{backup_restore,database,ddl_rest,expr,external,gap,hints,module_batch,query,statements,surface,util}.rs`
(in that expanded order), then `lib.rs`, `session.rs`, `tree.rs` and `wasm.rs`. The test module is excluded.
The rule-family split changes this identity even when the checker output is unchanged.
For each file it concatenates the full relative path, NUL, the UTF-8 content with CRLF normalized to LF, and NUL. It
also identifies rule changes that are not yet committed. Keep the repository commit together with the module SHA-256.
The identity is not a signature of the artifact and not a means of authenticating it.

## ABI version 2

The module exports `memory` and the functions below. On wasm32 every argument and return value is an **`i32` read as
an unsigned value**, and a pointer is a byte offset in the instance's linear memory. C strings, NUL terminators, shared
memory, callbacks and host memory pointers are not used. After creating the instance, call `_initialize` once if it
exists. There is no `_start`.

| Export | Arguments | Return value / behavior |
| --- | --- | --- |
| `tc_alloc` | `len` | A writable input pointer. `0` if allocation fails. Length 0 is supported. |
| `tc_free` | `ptr, len` | Frees an input buffer whose length matches the original allocation. A wrong combination is ignored. |
| `tc_check` | `source_ptr, source_len` | A JSON result handle. `0` for invalid input or an internal failure. |
| `tc_check_v2` | `source_ptr, source_len, quoted_identifier` | Initial session field: `0=ON`, `1=OFF`. Any other value gives handle `0`. |
| `tc_result_ptr` | `handle` | Byte offset of the read-only JSON. `0` for a missing or freed handle. |
| `tc_result_len` | `handle` | JSON length in bytes. `0` for a missing or freed handle. |
| `tc_result_free` | `handle` | Frees the result. A missing or freed handle is ignored. |
| `tc_identity` | none | An owned identity JSON result handle. |

Allocate a buffer, copy the source bytes into it, call `tc_check`, then free the input with `tc_free`. The existing
`tc_check` keeps the ON default. To check with OFF, pass `1` to `tc_check_v2`. The setting is independent for each call,
and a `SET QUOTED_IDENTIFIER` inside the source applies only within that source. The input pointer must be the start
address of a `tc_alloc` allocation that is not yet freed, and the input length must not exceed the length originally
requested. Both valid and invalid UTF-8 bytes are accepted as parser input. An SQL rejection is a non-zero handle that
holds `ok:false`. Handle `0` is an API failure, not an SQL verdict.

Look up the result pointer and length, copy and parse the JSON, then free it with `tc_result_free`. A result survives
freeing the input or running the next check, and several results can be held at the same time. The module never
modifies a live result. Result handles increase monotonically without reuse, so create a new instance after
`2^32-1` results have been produced. Do not modify result memory or use a freed input pointer. The allocator may reuse
the offset of a freed input.

Each check creates a new parser and tree and frees them before returning. Calls to one instance must be serialized; it
provides no re-entrancy or thread safety. Separate instances can run in parallel independently. Memory can grow during
allocation and checking, so obtain a new memory view after a module call before copying input or reading a result.
Freeing a buffer allows its memory to be reused but does not shrink the linear memory. The host must set a memory and
execution budget that fits the SQL it processes. OOM, a panic or a stack limit can raise a trap. After a trap, do not
assume anything about the allocator state; discard the instance.

## Per-input JSON

WASM returns the same per-input JSON as the CLI, but the caller manages file names, so the output has `path:""`. Every count and
offset is a non-negative integer. `row` and `column` start at 0, and column and ranges are in UTF-8 **bytes**, not
Unicode characters.

```json
{"path":"","ok":true,"root_has_error":false,"error_nodes":0,"missing_nodes":0,"zero_width_nodes":0,"root_end":9,"len":9,"first_problem":null,"constraint_violations":[]}
```

| Field | Meaning |
| --- | --- |
| `path` | The CLI input path. Always an empty string in WASM. |
| `ok` | Whether every check condition below passed. |
| `root_has_error` | The recovery/error flag of the tree-sitter root. |
| `error_nodes` | Number of `ERROR` nodes. |
| `missing_nodes` | Number of `MISSING` nodes. |
| `zero_width_nodes` | Number of zero-width nodes that are not missing, when the input is not empty. |
| `root_end`, `len` | Root end byte and input length in bytes. They must be equal for `ok`. |
| `first_problem` | `null`, or `{kind:string,row:integer,column:integer}` for the first tree problem in document order. |
| `constraint_violations` | An array of `{rule:string,detail:string,row:integer,column:integer}` in preserved order. Any entry makes `ok` false. |

`first_problem` covers tree problems only, so it can be `null` when only a constraint is violated. An empty source
follows the same verdict rules as the CLI. `ok` does not guarantee that the batch can run or that names, permissions
and types are correct.

For OFF, double-quoted strings are checked without modifying the original tree: the source is converted to an internal
check source of the same grammar. Error counts and kinds are computed on the internal tree, and every position and
length is mapped back to original bytes. For how the module captures settings and for the dynamic SQL boundary, see
[ADR-0006](decisions/ADR-0006-quoted-identifier-session.md).

## Engine agreement and the parity gate

A verdict from the grammar plus the constraint layer must pass every case the engine accepted in the committed SQL
Server oracle. It must reject every case the engine rejected, except the **explicitly classified exceptions** in
[`oracle/data/rejected-clean.json`](../oracle/data/rejected-clean.json). The exceptions are tree-constraint gaps, context, engine
version, environment and runner limits. This is a guarantee measured on the recorded oracle and compatibility levels,
not a guarantee for all possible T-SQL. For the exact baseline, see `oracle/data/expected.json` and the engine verdicts.
Adding WASM preserves that baseline and does not update it.

Verify checks incremental edits and the exception classification with the existing native oracle replay. It also
checks every input, in both ON and OFF, from `oracle/data/kit/cases.jsonl`, `oracle/data/probes/*.jsonl`, `test/fixtures/*.sql`,
the initial and intermediate sources of each edit history, and all ABI boundary cases, and compares every field of the
WASM and native JSON. Only the caller-managed path differs. A missing or extra case, a missing result and any JSON
difference are all failures. It also checks invalid pointers and handles, holding results at the same time, empty
input, Unicode, invalid UTF-8 and memory growth. The CI summary records batch time and import names. Native time
includes process start and file reads; WASM time includes allocation, the verdict and JSON parsing. The two figures are
measurements of each integrated path, not pure parser microbenchmarks.

## Connecting from a Go host without CGO

With **wazero**, instantiate WASI Preview 1 and this reactor. The checker needs no filesystem preopen, arguments or
environment variables. Configure reactor initialization so that `_start` is not run, and call the exported memory and
functions in the ownership order above. Treat `i32` values as `uint32`, and copy the JSON before freeing the handle.
Keep a separate module instance per worker and call it serially.

With **wasm2go**, convert the same reactor with the fixed translation pipeline of go-treesitter and provide the
remaining WASI imports. The result must preserve the growable linear memory, the exported functions and the reactor
initialization. This module contains Rust, the constraint rules, the tree-sitter C runtime and the allocator, so a
grammar-only WASM loader cannot replace it. On the converted Go host too, compare with the same inputs and JSON
baseline as `scripts/tree-check-wasm-parity.mjs`. This change does not implement or verify the Go-side connection.

## Remaining WASI imports

The default pinned build imports **only these 9 functions** from `wasi_snapshot_preview1`. It has no other module,
memory, table or global imports. This includes the call paths of the linked WASI libc and Rust runtime. Replacing the
two duplicate-check sets in the rules with `BTreeSet` removed the random seed and the `random_get` dependency. The JSON
verdict and the order of the violations array are unchanged.

| Function | WASI capability |
| --- | --- |
| `environ_get` | Read environment variable strings |
| `environ_sizes_get` | Read the environment variable count and buffer size |
| `clock_time_get` | Read the runtime clock |
| `fd_close` | Close a file descriptor |
| `fd_prestat_get` | Read preopen descriptor information |
| `fd_prestat_dir_name` | Read a preopen directory name |
| `fd_seek` | Change a descriptor position |
| `fd_write` | Write to an output descriptor; includes the runtime diagnostic path |
| `proc_exit` | Request process exit; not the return path of a normal SQL verdict |

We wrapped the imports in Node WASI to count calls. During `_initialize`, the identity lookup and the normal return of
all 12,009 inputs, **all 9 functions were called 0 times**. This is an observation on that input set, not a promise
that they are not called on a trap, panic or runtime failure. The host must connect these imports, and a normal check
needs no file permissions or preopen.
