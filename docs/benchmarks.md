# Benchmarks: 1.0.0, 2.0.0, 2.1.0 and 2.1.1

This page compares two versions of the grammar:

- **1.0.0**: the original grammar (alfpricot/tree-sitter-mssql v1.0.0) this repository started from, commit `852cbd2`, its committed
  `src/parser.c` and `src/scanner.c` as a consumer gets them. It has no constraint layer.
- **2.0.0**: this repository at commit `d206c0a` (the 2.0.0 candidate, `main` after pull requests #59–#62): the
  committed generated sources plus `crates/tree-check`, native release build and the WASM module.

This repository is independent and maintained by its owner. Everything below was measured on one machine in one
session, in alternating A/B rounds, and every figure says what it was measured on. Where the evidence ends, the text says so.

Contents: [Summary](#summary) · [Method](#method) · [Accuracy](#accuracy) · [Robustness](#robustness) ·
[Speed](#speed) · [Memory](#memory) · [Size](#size) · [2.1.0](#210) · [2.1.1](#211) ·
[Limits of these measurements](#limits-of-these-measurements)

Every section except [2.1.0](#210) and [2.1.1](#211) measures 2.0.0.

## Summary

| | 1.0.0 | 2.0.0 |
| --- | ---: | ---: |
| Engine-accepted oracle cases that parse without an error (15,585) | 10,056 (64.5 %) | 15,585 (100 %) with tree-check; 15,582 grammar alone¹ |
| Engine-rejected or runner-excluded oracle pairs flagged (5,833) | 3,083 (52.9 %), grammar error only | 5,388 (92.4 %) with tree-check; 1,749 (30.0 %) grammar alone |
| Private corpus (6,006 production scripts): accepted → clean | 5,874 / 5,987 | 5,987 / 5,987 |
| Private corpus: rejected → flagged | 8 / 19 | 18 / 19 |
| Parse throughput, private corpus, inputs both parse cleanly (B/ms, median of 5 rounds) | 11,438 | 9,958 (−13 %) |
| Parse tree bytes per source byte, private corpus | 24.6 | 25.7 (+4 %) |
| Largest single parse peak, private corpus (MiB) | 80.2 | 88.5 |
| `parser.c` (bytes) / compiled library, gcc `-O2`, stripped (bytes) | 30,368,933 / 10,349,056 | 28,423,048 / 13,448,704 |
| `IF … ELSE IF …` chain of 10,000 branches | 33.0 s | 0.06 s |
| `IF` nested without `BEGIN`, depth 10 and deeper | ERROR tree | clean tree |
| Incremental reparse equals a fresh parse (8,063 edits) | 8,063 | 8,063 |

¹ The 3 cases are QUOTED_IDENTIFIER OFF inputs with a double-quoted string; tree-check re-parses them under the
session's reading (ADR-0006), the grammar tree alone does not.

2.0.0 accepts everything SQL Server accepts in this oracle, catches most of what it rejects, and is linear on every
synthetic class measured. It parses about 13 % slower than 1.0.0 on clean input, uses about 4–5 % more memory, and its
compiled library is about 30 % larger.

## Method

**Host.** Intel Core Ultra 9 285H, 16 logical cores, 31.5 GiB RAM, Windows 11 Pro 10.0.26200. One session. Another
CPU-heavy job could run on the same machine, so system CPU load varied between 4 % and 47 % (sampled at the end of
each round or phase; see the table below). That is why every comparison between the two versions is an A/B run in
alternating rounds (the order flips each round) and every figure is a median. Absolute numbers on an idle machine may
be better; compare the ratios.

| Phase | Rounds | System CPU load at the end of each round or phase |
| --- | ---: | --- |
| Parser throughput A/B | 5 | 26 %, 31 %, 30 %, 32 %, 15 % |
| Incremental reparse A/B | 3 | about 4 % at the end |
| Native tree-check (2.0.0 only) | 3 passes | about 10 % at the end |
| WASM (2.0.0 only) | 1 | 9–13 % |
| Synthetic scaling | 3 (parse), 1 (memory, tree-check) | 13 %, 8 %, 13 % |

**Toolchain.** gcc 16.2.0 (MSYS2) with `CFLAGS=-O2`; rustc 1.100.0 (`stable-x86_64-pc-windows-gnu`), release profile;
Node v26.13.1; tree-sitter runtime 0.27.2 (`tree-sitter = "=0.27.2"`) and CLI 0.27.2; wasi-sdk 34.0 (clang 23.1.0), no
Binaryen (the unoptimised `rust-O3-lto` WASM recipe of `scripts/tree-check-wasm-build.mjs`). Every cache was isolated in
a scratch folder; nothing was installed.

**Subjects.** Two checkouts, `852cbd2` (1.0.0) and `d206c0a` (2.0.0). Both are linked through their own Rust binding
(`bindings/rust`, which compiles `src/parser.c` and `src/scanner.c`) into the same benchmark program, so the 1.0.0 and
2.0.0 parsers run the same measuring code. The program has these modes: `parse` (time and tree shape), `mem` (a counting
allocator installed with `ts_set_allocator`), `peak` (process peak working set, one fresh process per sample), `verdict`
(parse problems only), `edits` (edit histories), and `incr` (incremental reparse). 2.0.0's tree-check numbers come from
a build of `crates/tree-check` with the same helper code (`check` mode: parse time and inspect time per input). The WASM
module was built with `node scripts/tree-check-wasm-build.mjs` and driven from Node's WASI with the `tc_check_v2` export
(one warm instance, three samples per input; memory with a fresh instance per input).

**Timing.** Per input: a warm-up call, then 5 samples (parser) or 3 samples (incremental, WASM, synthetic), each sample
long enough (about 2 ms or more) to beat timer resolution; the sample median is the input's time. Across rounds the
per-input median is used. Throughput is total bytes divided by total time. "Both parse cleanly" is the subset of inputs on
which both versions produce a tree with no ERROR or MISSING node, no zero-width node and a root that spans the input
(`tree-check`'s parse conditions, without the constraint rules). Error recovery is slower than a clean parse, so each
speed and memory comparison is given on all inputs and on that subset.

**Inputs.**

| Set | Content |
| --- | --- |
| `fixtures` | the 156 files of `test/fixtures` (120,099 bytes) |
| `oracle` | the 10,709 sources of `oracle/data/kit/cases.jsonl` and `oracle/data/probes/*.jsonl` (1,421,602 bytes), one file each |
| private corpus | 6,006 production T-SQL scripts, 50,748,966 bytes (size bands: <1 KB 1,760 files; 1–10 KB 3,306; 10–100 KB 882; 100 KB–1 MB 56; >1 MB 2). Read locally only; this page gives aggregate numbers only |
| synthetic | 41 generator classes (`in_list`, `and_chain`, `case_nested`, `if_else_chain`, …), each grown geometrically from 1 KB to at most 4 MB (nested classes to 128 KB), 437 files |

**Accuracy reference.** SQL Server `SET PARSEONLY ON` (the oracle of `oracle/data`, see `oracle/README.md`): LocalDB
17.0.1000.7 and the four Linux engines (2017 CU31, 2019 CU32, 2022 CU28, 2025 CU11), compatibility levels 100–170
where the engine supports them, both QUOTED_IDENTIFIER sessions. A case is *accepted* when any recorded engine and level
accepts it under the same session setting, as in `scripts/oracle-replay.mjs`, whose merge code the accuracy script
imports. The private corpus's verdicts come from LocalDB 17.0.1000.7, levels 100–170, QUOTED_IDENTIFIER ON; each
verdict file's `cases_sha256` was checked against the exact files measured.

**Commands** (the benchmark program and scripts are not yet part of this repository; they are plain Cargo and Node
scripts around the commands below):

```sh
CFLAGS=-O2 cargo build --release            # per subject, in its own checkout and target directory
node scripts/oracle-replay.mjs --tree-check <tree-check.exe>      # the 2.0.0 oracle gate (2.0.0 only)
tree-sitter test                            # corpus tests, CLI 0.27.2; one TREE_SITTER_LIBDIR per subject
node scripts/tree-check-wasm-build.mjs      # WASM module (WASI_SDK_PATH = wasi-sdk 34.0)
gcc -O2 -std=c11 -I src -c src/parser.c src/scanner.c ; gcc -shared -s -o mssql.dll parser.o scanner.o
```

## Accuracy

### Oracle

21,418 case/session pairs (10,709 sources under both QUOTED_IDENTIFIER settings): 15,585 accepted, 5,831 rejected and 2 runner-excluded. The negative column below follows the replay:
5,833 not-accepted pairs, including the two exclusions.

| | 1.0.0 | 2.0.0 grammar alone | 2.0.0 grammar + tree-check |
| --- | ---: | ---: | ---: |
| Accepted that parse without an error | 10,056 (64.5 %) | 15,582 (99.98 %) | 15,585 (100 %) |
| Rejected or runner-excluded that are flagged | 3,083 (52.9 %) | 1,749 (30.0 %) | 5,388 (92.4 %) |
| Accepted clean + rejected/excluded flagged, all pairs | 13,139 (61.3 %) | 17,331 (80.9 %) | 20,973 (97.9 %) |

By session: QUOTED_IDENTIFIER ON has 7,838 accepted, 2,870 rejected and 1 runner-excluded case; OFF has 7,747, 2,961 and 1. 2.0.0 with
tree-check accepts all engine-accepted cases in both sessions; its grammar alone misses 3 OFF cases (see the summary note).

The 445 rejected or runner-excluded pairs that 2.0.0 does not flag are all classified in `oracle/data/rejected-clean.json`: 111 `context`
(need declarations or the catalog), 36 `version` (a feature these engine builds lack), 294 `environment` (edition,
platform or server configuration), 4 `runner` (not judgeable), 0 `gap` (decidable from the tree but not enforced).

1.0.0 flags more rejected cases than the grammar alone of 2.0.0 because it also puts errors into many inputs SQL Server
accepts; its 3,083 flags are not evidence of better rejection.

Per case set (accepted cases that parse cleanly / accepted; rejected or runner-excluded flagged / not accepted):

| Set | 1.0.0 clean | 2.0.0 clean | 1.0.0 flagged | 2.0.0 flagged (grammar + tree-check) |
| --- | ---: | ---: | ---: | ---: |
| `kit` (grammar-kit cases) | 660 / 840 | 840 / 840 | 182 / 612 | 608 / 612 |
| `surface` | 5,780 / 7,257 | 7,257 / 7,257 | 417 / 761 | 436 / 761 |
| `kit-routes` | 491 / 1,177 | 1,177 / 1,177 | 66 / 103 | 76 / 103 |
| `context` | 1,180 / 1,182 | 1,182 / 1,182 | 2 / 412 | 412 / 412 |
| `hint-rules` | 768 / 946 | 946 / 946 | 40 / 246 | 246 / 246 |
| `expr-forms` | 54 / 492 | 492 / 492 | 208 / 280 | 268 / 280 |
| `ddl-rest` | 94 / 655 | 655 / 655 | 339 / 411 | 407 / 411 |
| `backup-restore` | 166 / 444 | 444 / 444 | 538 / 888 | 888 / 888 |
| `gap-forms` | 177 / 239 | 239 / 239 | 100 / 459 | 442 / 459 |
| `query-forms` | 42 / 216 | 216 / 216 | 104 / 132 | 132 / 132 |
| `ddl-database` | 64 / 299 | 299 / 299 | 199 / 219 | 214 / 219 |
| `ddl-external` | 28 / 173 | 173 / 173 | 191 / 195 | 187 / 195 |
| `quoted-identifier` | 118 / 203 | 203 / 203 | 3 / 41 | 37 / 41 |
| `if-nesting` | 38 / 70 | 70 / 70 | 8 / 8 | 8 / 8 |
| the other 19 sets | 396 / 1,392 | 1,392 / 1,392 | 686 / 1,066 | 1,027 / 1,066 |
| **total** | **10,056 / 15,585** | **15,585 / 15,585** | **3,083 / 5,833** | **5,388 / 5,833** |

The oracle cases were written by this project while it fixed defects, so 2.0.0 was developed against them. Read the
2.0.0 column as a regression gate, not as an independent test; the 1.0.0 column measures 1.0.0 against cases written
for 2.0.0.

### Corpus tests and fixtures

| | 1.0.0 | 2.0.0 |
| --- | ---: | ---: |
| `test/fixtures` (156 files) that parse without an error | 155 | 156 |
| `tree-sitter test` on the 2.0.0 corpus (`test/corpus`, 2,562 parses) | 814 pass (31.8 %) | 2,562 pass |
| `tree-sitter test` on 1.0.0's own corpus | 370 / 370 | not run |

The 2.0.0 corpus pins 2.0.0's tree shapes, so most of the 1.0.0 failures are different trees for the same input or
inputs that 1.0.0 cannot parse; this row says how far the two grammars are apart, not how many are wrong.

### Private corpus

6,006 scripts against the stored SQL Server verdicts (5,987 accepted, 19 rejected):

| | 1.0.0 | 2.0.0 grammar alone | 2.0.0 grammar + tree-check |
| --- | ---: | ---: | ---: |
| Accepted → clean | 5,874 / 5,987 | 5,987 / 5,987 | 5,987 / 5,987 |
| Rejected → flagged | 8 / 19 | 5 / 19 | 18 / 19 |
| Files with a root error | 121 (2.0 %) | 5 (all 5 are engine-rejected) | 5 |
| Disagreements | 124 | 14 | 1 |

The one 2.0.0 disagreement is an engine rejection with a context error number (137, an undeclared variable), which a
grammar cannot decide. This corpus comes from a single source and was also used while the grammar was developed; it
is a real-world check, not a held-out test set.

## Robustness

Each input in its own process, 120 s limit. No crash, hang or timeout of 2.0.0 or of its tree-check on any of them.
`parse` is the grammar alone (ms inside the process); "err" means the tree has an ERROR.

| Input | 1.0.0 parse | 2.0.0 parse | 2.0.0 tree-check (whole process) |
| --- | --- | --- | --- |
| `IF` nested without `BEGIN`, depth 10 / 50 / 200 | err 0.2 / 0.5 / 1.6 ms | clean 0.17 / 0.29 / 0.75 ms | clean, 12–26 ms |
| same, depth 1,000 / 8,192 | err 8.0 / 75 ms | clean 3.5 / 27 ms | clean, 20 / 181 ms |
| `IF … ELSE IF …` 1,000 / 10,000 / 50,000 branches | 0.31 s / **33.0 s** / over 120 s | 8 ms / 61 ms / 306 ms | 32 ms / 0.17 s / 0.82 s |
| nested parentheses 100 / 1,000 / 10,000 / 50,000 | 0.18 / 0.74 / 6.4 / 29.0 ms | 0.20 / 0.73 / 6.0 / 29.8 ms | 13–114 ms |
| nested `CASE` 100 / 1,000 / 10,000 / 20,000 | 0.6 / 4.3 / 40 / 82 ms | 0.6 / 4.8 / 44 / 90 ms | 16–206 ms |
| nested subqueries 10 / 100 / 1,000 / 5,000 | 0.23 / 0.53 / 3.9 / 19 ms | 0.25 / 0.59 / 4.2 / 21 ms | 12–69 ms |
| nested function calls 100 / 1,000 / 10,000 / 20,000 | 0.5 / 2.4 / 23 / 48 ms | 0.5 / 3.0 / 29 / 60 ms | 16–293 ms |
| nested `BEGIN`/`END` 10,000, `WHILE` 5,000, `TRY`/`CATCH` 5,000 | 7.8 / 17 / 24 ms | 8.1 / 18 / 25 ms | 34 / 57 / 82 ms |
| 200,000-element `IN` list (about 1.2 MB) | 251 ms | 253 ms | 0.52 s |
| 50,000-column select list | 75 ms | 77 ms | 0.23 s |
| 100,000-term `AND` chain | 214 ms | 220 ms | 0.63 s |
| 200,000-term string concatenation | 165 ms | 207 ms | 0.53 s |
| 100,000-row `INSERT … VALUES` (3.3 MB) | 375 ms | 429 ms | 0.78 s |
| 200,000 statements | 523 ms | 539 ms | 1.5 s |
| one 8 MB string literal | 20 ms | 21 ms | 37 ms |

Scaling exponent (slope of log time against log size, points of 16 KB and more; 1.0 is linear), parse, 41 classes: 2.0.0
0.96–1.06 in every class (`if_else_chain` 1.00). 1.0.0 is 0.92–1.06 except `if_else_chain`, where it is 1.99 (quadratic:
252 KB takes 18 s; larger sizes exceeded the 90 s limit). Parse-memory exponent: 0.93–1.00 in every class, for both (0 for
the string and comment classes, whose memory does not depend on size). tree-check (`inspect` time) on 2.0.0: 0.51–1.08;
the highest is nested `CASE` (1.08), no class shows a quadratic trend. At equal size the 2.0.0 parse takes 0.4–1.4 times
as long as the 1.0.0 parse per class (median 1.04, 40 classes without `if_else_chain`). The robust inputs and the
synthetic classes are valid T-SQL for the grammar; SQL Server's own nesting limits were not tried.

## Speed

Parse throughput, B/ms (median of 5 alternating rounds, per-input median, total bytes over total time).

| Input | Files | 1.0.0, all | 2.0.0, all | 1.0.0, both clean | 2.0.0, both clean | Both clean: files |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| fixtures | 156 | 11,435 | 9,857 | 11,480 | 9,895 | 155 |
| oracle | 10,709 | 4,935 | 8,067 | 8,202 | 8,627 | 6,342 |
| private corpus | 6,006 | 11,124 | 9,913 | 11,438 | 9,958 | 5,885 |
| private, <1 KB | 1,760 | 19,222 | 15,383 | 19,296 | 15,392 | 1,758 |
| private, 1–10 KB | 3,306 | 13,714 | 11,447 | 13,871 | 11,483 | 3,237 |
| private, 10–100 KB | 882 | 11,421 | 10,360 | 11,973 | 10,389 | 840 |
| private, 100 KB–1 MB | 56 | 9,950 | 8,847 | 10,118 | 8,883 | 48 |
| private, >1 MB | 2 | 8,226 | 7,742 | 8,226 | 7,742 | 2 |

- On clean private-corpus input 2.0.0 parses about 6–20 % slower than 1.0.0 (2.0.0 over 1.0.0: 0.80 below 1 KB, 0.83 at 1–10 KB,
  0.87 at 10–100 KB, 0.88 at 100 KB–1 MB, 0.94 above 1 MB). Per file (files of 10 KB and more, both clean, 890 files),
  the median ratio of 2.0.0 throughput to 1.0.0 throughput is 0.85 (10th percentile 0.74, 90th percentile 0.96).
- Cause of the remaining difference. Since pull request #62 the generated `parser.c` is compiled with optimization:
  `tree-sitter generate` 0.27.2 emits `#pragma GCC optimize ("O0", "jump-tables")` for a whole `parser.c` once the main lexer has more
  than 300 states, and 2.0.0's lexer had 568; it now has 271, and the parse-state count fell from 15,020 to 14,107. Before
  that change the same measurement gave about 30 % (the corpus ratio was 0.70 against 0.87 now). An earlier counter study attributes the remaining gap to
  the larger grammar: in a deterministic counter study on inputs both versions parse
  cleanly, lexer calls and shifts per KB were the same, and 2.0.0 was slower on the same work because of larger parse
  tables (4,478 large states over 1,368 symbols instead of 4,969 over 913), Unicode identifier sets (`set_contains`
  ranges, about 3 % of throughput on the corpus), more reductions (+4 to +11 % per KB, from new hidden wrapper
  nodes) and more entries scanned in the small tables (+14 to +56 % for tokens), plus more characters skipped by the
  external scanner (+5 to +17 %). These shares were not separated one by one, and the counters were not re-taken for
  this page.
- On the oracle, 2.0.0 is faster on all inputs because 1.0.0 spends time recovering from errors on the many valid
  inputs it cannot parse (4,306 of 10,709 oracle sources have a root error in 1.0.0, 876 in 2.0.0). On the 6,342 sources
  both parse cleanly, 2.0.0 is 5 % faster; the oracle's inputs are tiny (133 bytes on average), so fixed costs dominate.
- Slowest single parse in the private corpus (the largest files are 2.25 MB): 284 ms with 1.0.0, 299 ms with 2.0.0.
- Real throughput for planning: 2.0.0 parses a typical production script at about 10 MB/s on this machine (8–15 MB/s by
  size), without tree-check.

### Incremental reparse

Edits on a prior tree, each applied on its own: a no-op replacement, one inserted space, one inserted letter, one
replaced token, each at 5–6 positions of a file (start, quarter, middle, three quarters, end, the middle of the
largest procedure). Median of 3 alternating rounds per edit.

| Input (edits that leave a clean tree in both) | Version | Median ms | p95 ms | Ratio to a fresh parse: median / p95 / max |
| --- | --- | ---: | ---: | --- |
| fixtures (3,146 edits) | 1.0.0 | 0.0063 | 0.036 | 0.27 / 0.80 / 1.31 |
| | 2.0.0 | 0.0066 | 0.046 | 0.27 / 0.80 / 2.52 |
| 25 files of the private corpus, 4 KB–2.25 MB (528 edits) | 1.0.0 | 0.29 | 5.3 | 0.023 / 0.28 / 0.46 |
| | 2.0.0 | 0.55 | 5.2 | 0.071 / 0.41 / 0.44 |
| synthetic, 228 inputs (3,228 edits) | 1.0.0 | 0.066 | 1.9 | 0.10 / 0.70 / 1.16 |
| | 2.0.0 | 0.075 | 2.3 | 0.12 / 0.59 / 1.13 |

For the private files of more than 1 MB (40 edits) the medians are 5.4 ms (1.0.0) and 5.4 ms (2.0.0) against about
210–240 ms for a fresh parse. In the 100 KB–1 MB band the median is 0.29 ms (1.0.0) against 0.50 ms (2.0.0), and in the
10–100 KB band 0.15 ms against 0.54 ms: on real files an incremental reparse with 2.0.0 takes about 1.9 times as long
as with 1.0.0, and still about 14 times less than a fresh parse in the median.

**2.1.1.** Re-measured with the same edits for all three versions.

- **Method.**
  - The edits are chosen once, on 2.1.1's tree, so 1.0.0, 2.0.0 and 2.1.1 get byte-identical edits.
  - One process loads all three grammars, interleaved, on a pinned core.
  - Each edit is the median of 5 samples; there are 2 runs, 3 for synthetic.
  - The edit sets are the same: 25 private files (528 clean edits, 24 files), fixtures and synthetic.
- **Ratios over 1.0.0.**

  | Edits | 2.0.0 per edit / summed | 2.1.1 per edit / summed |
  | --- | ---: | ---: |
  | private, all | 1.42 / 1.35 | 0.986 / 1.002 |
  | private, 10–100 KB | 1.62 / 1.87 | 0.999 / 1.022 |
  | private, 100 KB–1 MB | 1.01 / 2.36 | 0.980 / 1.104 |
  | private, over 1 MB | 0.99 / 0.98 | 0.979 / 0.969 |
  | fixtures | 1.02 / 1.19 | 0.962 / 0.870 |
  | synthetic | 1.03 / 1.19 | 1.012 / 1.028 |

- **Why 2.0.0 was slower.** In 2.0.0 the procedure-body fork (#60) kept up to 4 GLR versions. Nodes built under more
  than one version are fragile and are not reused, so every edit re-parsed procedure bodies. #68 settles that fork at
  generation time.
- **What is left in 2.1.1.** It is concentrated in a few files: one file sums to 2.7 times 1.0.0's time.
  - Cause: the external tokens for spaced comparison operators (`< =` and the like) are valid only after an
    operand, so they split the external lexer states.
  - Effect: after a statement that ends in an operand without `;`, the next statement's first token was lexed in the
    "after an operand" state. An incremental reparse reaches it from "after a batch item" and cannot reuse it, so the
    statement is rebuilt.
- **A grammar fix that was tried and not adopted.** Valid everywhere as a dead-end non-terminal extra, these tokens
  stop the split.
  - Incremental reparse: 10–100 KB 0.88, 100 KB–1 MB 0.91 over 1.0.0, with exactness kept.
  - Cost: it slows full parses of the corpus tests by 6 %, and grows `parser.c` by 19 % and the large states by 31 %.
- **A runtime fix (not upstream at the time of writing).** `can_reuse_first_leaf` could compare only the external
  tokens that can start at that byte; a diagnostic runtime copy shows the same gain.
- **Common to all three versions.** These costs are the same in 1.0.0:
  - top-level repeat nodes are fragile, so every edit walks every top-level item: about 8,600 steps and 6.8 ms per
    edit in files over 1 MB;
  - `ts_tree_get_changed_ranges` takes about 70 % of that again.

**Exactness.** An incremental tree should equal a fresh parse. Measured with tree-sitter 0.27.2:

| Edit set | Edits | Differ from a fresh parse, 1.0.0 | 2.0.0 |
| --- | ---: | ---: | ---: |
| oracle edit histories (`oracle/data/kit/*edits.jsonl`, cumulative) | 1,111 steps in 438 histories | 0 | 0 |
| fixtures | 3,172 | 0 | 0 |
| synthetic | 3,228 | 0 | 0 |
| 25 private files | 552 | 0 | 0 |

An earlier 2.0.0 candidate (`920d97a`) differed from a fresh parse on 24 of the 552 edits of one 244 KB private file.
Pull request #60 makes a procedure or trigger body run to the next `GO` by a strict dynamic-precedence score, so an
incremental reparse now equals a fresh parse; the difference is gone. This is a measurement on these edit sets with
tree-sitter 0.27.2, not a proof: the edits are four simple kinds applied to 25 real files plus the public inputs.
tree-sitter 0.27.3 is not released at the time of writing (crates.io lists 0.27.2 as newest). Its release branch has a
merged pull request (tree-sitter#6152, merged 2026-11-16) with four incremental-parsing fixes (a node ended by a
zero-width token is no longer reused, lex-mode comparison without `memcmp`, a word token is not reused under a state
with different reserved words, and the keyword fallback applies to a reused node's first leaf). Those are runtime
defects independent of this grammar; this page did not test 0.27.3.

**How much is re-lexed per edit.** A separate study of the incremental-reuse map (217,563 edits plus 376 supplementary
ones, all from public inputs; deterministic counters against the `main` of before pull request #62) measured the
effect of the reuse repairs on the number of tokens the runtime re-lexes:

- Re-lexed tokens over all edits: −8.7 %.
- Edits that re-lex more than 4 tokens beyond the reference build of that study: 320 → 314; of those, edits whose text is
  clean before and after: 74 → 16.
- Known cost: 52 edits that leave the text invalid re-lex more than before, by more than 4 tokens (at most +28).

### tree-check (2.0.0 only; 1.0.0 has no constraint layer)

Native release build, parse plus inspect (constraints and the QUOTED_IDENTIFIER projection), median of 3 passes:

| Input | Parse ms | Inspect ms | Parse + inspect, B/ms | Inspect alone, B/ms | ms per MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| fixtures | 8.9 | 16.7 | 4,691 | 7,185 | 213 |
| oracle | 134.6 | 252.2 | 3,675 | 5,637 | 272 |
| private corpus (50.7 MB) | 4,105 | 5,757 | 5,146 | 8,815 | 194 |

Inspect time shows no quadratic trend in any of the 41 synthetic classes (exponent 0.51–1.08). WASM module (`tc_check_v2`,
Node 26.13.1, one warm instance; each call includes parse, inspect and the result): fixtures 3,431 B/ms, oracle 2,951,
private corpus 4,388 B/ms (11.6 s for 50.7 MB). The native and WASM runs were not alternated, and the native benchmark
build has no LTO while the WASM module does; the WASM time was 1.17 times the native time on the private corpus (1.37 on
fixtures, 1.25 on the oracle). Read that as "WASM takes about 1.2 times as long as native", not as a precise figure.
Module compile 8 ms, instantiation 21 ms.

## Memory

Counted with an allocator installed through `ts_set_allocator` (deterministic). "Tree" is the bytes retained by the
tree; "peak" the maximum live bytes during the parse; both per source byte.

| Input | Tree, 1.0.0 | Tree, 2.0.0 | Peak, 1.0.0 | Peak, 2.0.0 | Max peak, 1.0.0 → 2.0.0 |
| --- | ---: | ---: | ---: | ---: | --- |
| fixtures | 23.5 | 24.7 | 28.4 | 29.8 | 0.15 → 0.16 MiB |
| oracle | 32.7 | 32.0 | 57.4 | 54.2 | 0.53 → 0.51 MiB |
| private corpus | 24.6 | 25.7 | 25.5 | 26.8 | 80.2 → 88.5 MiB |
| private, both clean (5,885 files) | 24.7 | 25.8 | 25.7 | 27.0 | |
| private, <1 KB | 15.2 | 15.5 | 23.2 | 25.6 | |
| private, 1–10 KB | 19.3 | 19.5 | 21.7 | 22.5 | |
| private, 10–100 KB | 21.4 | 21.9 | 21.9 | 22.5 | |
| private, 100 KB–1 MB | 31.0 | 32.8 | 31.1 | 32.9 | |
| private, >1 MB | 37.3 | 41.1 | 37.3 | 41.2 | 80.2 → 88.5 MiB |

Process peak working set of one parse in a fresh process (net of the working set before the parse; median of 3; an
empty process starts at 4.77 MiB; bytes per source byte, and the largest sample):

| Input | 1.0.0 parse | 2.0.0 parse | 2.0.0 parse + tree-check |
| --- | ---: | ---: | ---: |
| private, 10–100 KB | 73 B/B, max 3.1 MiB | 81 B/B, max 3.4 MiB | 91 B/B, max 3.7 MiB |
| private, 100 KB–1 MB | 43 B/B, max 40.3 MiB | 46 B/B, max 42.9 MiB | 51 B/B, max 52.1 MiB |
| private, the 5 largest (0.58–2.25 MB) | 42 B/B, max 92.8 MiB | 46 B/B, max 102.1 MiB | 49 B/B, max 102.3 MiB |

WASM (2.0.0): initial linear memory 14,483,456 bytes (13.8 MiB). Memory grows with the input and never shrinks, so the
total after the largest check is the peak: 14.0 MiB for under 10 KB inputs, 15.1 MiB for 10–100 KB, 57.8 MiB for
100 KB–1 MB, and 107.8 MiB for the 2.25 MB script (growth about 43.7 bytes per source byte above 1 MB).

## Size

| | 1.0.0 | 2.0.0 |
| --- | ---: | ---: |
| `src/parser.c` | 30,368,933 B | 28,423,048 B (−6.4 %) |
| `src/scanner.c` | 4,706 B | 12,646 B |
| parse states / of which large | 15,192 / 4,969 | 14,107 / 4,478 |
| symbols / token types / external tokens | 913 / 453 / 6 | 1,368 / 628 / 21 |
| named and anonymous node types in `node-types.json` | 720 | 964 |
| `parser.o` (gcc 16.2.0 `-O2`) | 10,368,630 B | 13,481,505 B |
| shared library, parser + scanner, stripped | 10,349,056 B | 13,448,704 B (+30 %) |
| WASM module (tree-check, no Binaryen) | n/a | 13,980,776 B |

Node kinds and fields are the public contract: against 1.0.0, 0 node kinds and 0 fields are removed or renamed and 244
kinds are added.

## 2.1.0

2.1.0 is `main` after pull request #65 (`693a575`). Its only change from 2.0.0 is a performance change in the grammar
and `src/scanner.c`, so a consumer that regenerates `parser.c` with CLI 0.27.2 gets all of it.

**Unchanged from 2.0.0.**

- `node-types.json`, node kinds, fields, and full trees (named and anonymous nodes, ranges, flags) on every valid input.
  This was checked on the fixtures, oracle, corpus tests, synthetic inputs, random procedures and the private corpus.
- Verdicts and incremental exactness.
- One tree changes, on an engine-rejected input (`BEGIN CREATE PROCEDURE p AS SELECT 1; END`): the final `;` moves
  from the procedure body to the block.

**Method.** This study did not use the alternating rounds of the sections above. Instead:

- One process loads one static tree-sitter 0.27.2 runtime and the three grammars, all built with gcc `-O2`. The
  1.0.0 `parser.c` is identical to its CLI 0.27.2 regeneration.
- It alternates the variant order per input and per sample. Each sample repeats for at least 2 ms, and the result is
  the median of 5 samples (3 on the oracle).
- The process is pinned to one performance core.
- The study ran twice, and every set ratio agreed within 0.003.

The figures are not directly comparable with the [Speed](#speed) table, which used separate executables. Inputs are
those all three versions parse cleanly.

| Set | Inputs | 1.0.0 (B/ms) | 2.0.0 / 1.0.0 | 2.1.0 / 1.0.0 | 2.1.0 per-input median |
| --- | ---: | ---: | ---: | ---: | ---: |
| fixtures | 154 | 19,110 | 0.878 | 0.991 | 1.005 |
| oracle | 6,342 | 13,514 | 1.003 | 1.105 | 0.976 |
| corpus tests | 740 | 14,709 | 0.884 | 0.973 | 0.994 |
| synthetic¹ | 39 | 11,387 | 0.949 | 1.010 | 1.004 |
| private corpus | 5,874 | 17,147 | 0.877 | 0.987 | 0.952 |
| private, <4 KB | 4,090 | 22,486 | 0.848 | 0.956 | |
| private, 4–16 KB | 1,248 | 20,475 | 0.849 | 0.967 | |
| private, 16–64 KB | 457 | 18,237 | 0.876 | 0.986 | |
| private, 64–256 KB | 69 | 16,204 | 0.890 | 0.990 | |
| private, ≥256 KB | 10 | 12,438 | 0.899 | 1.011 | |

¹ Without the `IF … ELSE IF …` chain, where 1.0.0 is quadratic.

**What changed, and its effect.**

| Change | Effect |
| --- | --- |
| Identifier, `@variable` and `#temp` patterns send ASCII and non-ASCII characters to different lexer states | ASCII is tested with plain comparisons; the Unicode range table is binary-searched only after a non-ASCII character ([tree-sitter#6139](https://github.com/tree-sitter/tree-sitter/issues/6139)). Main lexer cycles fell to about 0.8 times 1.0.0's on synthetic input |
| Space, LF, CR and tab repeated as extras with distinct precedences | Each gets its own single-character transition ahead of the 53-code-point separator set |
| `_literal_string` no longer aliased | Saves one reduction and one tree node per string literal |
| `prec.right` on a procedure body's later items | No GLR fork at every statement boundary |
| Scanner fast exit | The scanner returns before any check when no external token can start |

On the private corpus, reductions fall from +11.7 % to +4.2 % per KB over 1.0.0, and GLR version steps from +4.9 % to
+2.6 %. Parse states fall from 14,107 to 14,074, and the main lexer has 274 states.

**Where the gap remains.** Private inputs that need no more GLR steps than in 1.0.0 parse at 1.007 times 1.0.0's
speed. The gap grows with the extra work:

| Version steps over 1.0.0 | Ratio |
| --- | ---: |
| 0.1–3 % | 0.998 |
| 3–6 % | 0.960 |
| 6–10 % | 0.943 |
| more than 10 % | 0.90 |

The extra work comes from three sources:

- Hidden wrapper rules (`_batch_item`, `_invocation_arguments`, `_expression_list`, `_case_body`,
  `_query_specification_rest`, …). Inlining them grows the parse table by 3–12 %, and for some it worsens
  incremental reuse.
- The remaining procedure-body conflict.
- The scanner reading whitespace before the main lexer reads it again, which costs 0–3 %. The runtime calls the
  external scanner first for every token, and a nested block comment cannot be found without skipping whitespace.

**Memory and size.**

- Tree bytes per source byte over 1.0.0:

  | Set | 2.0.0 | 2.1.0 |
  | --- | ---: | ---: |
  | private corpus | +4.5 % | +0.3 % |
  | fixtures | +5.3 % | +3.2 % |
  | oracle | +6.0 % | +4.4 % |

  These are counted with `ts_set_allocator` and are deterministic.
- Peak during a parse on the private corpus: +5.2 % over 1.0.0 in 2.0.0, +1.6 % in 2.1.0.
- The gcc `-O2` library is 0.4 % smaller than 2.0.0's (+29 % over 1.0.0), and `parser.c` is 0.5 % smaller.

**clang.** A wasi-sdk 34.0 clang `-O2` wasm32-wasip1 build under Node, pinned, over 3 alternating rounds, points the
same way: 2.1.0 is 5–7 % faster than 2.0.0. It is also 0.94–0.99 times 1.0.0, a larger gap than with gcc. That run
was not warmed up; [2.1.1](#211) isolates the cause.

## 2.1.1

2.1.1 is `main` after pull request #68 (`aa4287d`). It changes only the grammar.

- **Procedure-body forks.**
  - 2.1.0 settled two forks at run time by dynamic precedence: an empty body against a statement after `AS`, and
    ending the body after its first item.
  - Static precedence now settles both. `AS` is written per alternative, because the generator compares the
    precedence of the step just before the dot.
  - Only the leading-semicolon fork stays.
- **Unit reductions.**
  - OPENROWSET's first connection value spells out `literal`'s alternatives.
  - `literal` takes the string token directly.
  - Both let the generator drop unit reductions it kept before.

Valid-input trees, node kinds, fields and verdicts are unchanged. Inputs SQL Server rejects can get a different tree;
the [CHANGELOG](../CHANGELOG.md) lists those.

The method is the same as for [2.1.0](#210): two interleaved runs, inputs all versions parse cleanly. The ratios
below are over 1.0.0.

| Set | 2.1.0 | 2.1.1 | 2.1.1 inputs faster than 1.0.0 |
| --- | ---: | ---: | ---: |
| fixtures | 0.983 | 1.022 | 66 % |
| oracle (total / per-input median) | 1.105 / 0.974 | 1.152 / 0.984 | 37 % |
| corpus tests | 0.967 | 0.984 | 48 % |
| synthetic | 0.995 | 1.009 | 64 % |
| private corpus | 0.985 | 1.017 | 88 % |
| private, <4 KB / 4–16 / 16–64 / 64–256 / ≥256 KB | 0.950 / 0.965 / 0.985 / 0.994 / 1.007 | 1.027 / 1.023 / 1.017 / 1.013 / 1.011 | |

The 2.1.0 column is this run's measurement and agrees with the [2.1.0](#210) table within 0.01.

**Counters.** On the private corpus, against 1.0.0:

- GLR multi-version steps are +0.17 % (2.1.0: +5.9 %).
- Shifts and lexer calls are equal.
- Reductions are +2.6 % (2.1.0: +4.4 %).

**Where the gap remains.**

- **Corpus tests (−1.6 %).**
  - Corpus-test inputs with as many reductions as in 1.0.0 parse at 1.03 times 1.0.0's speed. Inputs with 10 % more
    reductions parse at 0.89.
  - The extra reductions come from hidden wrappers such as `_invocation_arguments`, `_expression_list` and
    `_query_specification_rest`.
  - Seven restructurings were tried. All but the two now in 2.1.1 grew the parse table (by up to 11 %) or made
    incremental reuse worse (2 to 2,843 clean edits).
- **Most GLR steps.** The largest share is the qualified column against object reference after `.`, the same fork as
  in 1.0.0. Settling it statically would change the tree shape, which is a breaking change.

**clang/WASM.** This build uses wasi-sdk 34 clang `-O2`, wasm32-wasip1 under Node, measured after a warm-up pass.

| Set | 2.1.0 | 2.1.1 |
| --- | ---: | ---: |
| fixtures | 0.919 | 0.972 |
| corpus tests | 0.939 | 0.946 |
| synthetic | 0.948 | 0.979 |

The lexer calls were replayed alone (lexer functions only, fixtures and corpus tests):

- With gcc, the main lexer costs 0.70 times 1.0.0's.
- In WASM, it costs 1.14 times 1.0.0's.
- Most of the WASM difference is one `set_contains` binary search over the 368-range Unicode identifier table, run
  for the character that ends each identifier. The generator draws no ASCII check before it
  ([tree-sitter#6139](https://github.com/tree-sitter/tree-sitter/issues/6139)).
- A diagnostic copy of the generated `parser.c` adds `lookahead >= 0x80 &&` in front of the 24 such calls whose ASCII
  characters were already handled. With it, the WASM main lexer costs 0.995 times 1.0.0's.
- A grammar cannot express that check. A consumer that post-processes the generated file, or generates it with a CLI
  that has an ASCII fast path, gets the rest of the speed.
- That post-processing was tried, with a 128-bit ASCII bitmap per range table so that every call keeps its exact
  result. Every tree stayed identical. On whole parses it saved 1.2–1.7 % in WASM (within the run-to-run range) and
  nothing with gcc, because the lexer is about a fifth of parse time. It was not adopted.

## Limits of these measurements

- One machine, one operating system (Windows), one session, possibly with another CPU-heavy job running.
- The private corpus is one source, 6,006 scripts of which 19 are engine-rejected; it was also used while the grammar
  was developed. The oracle cases were written by this project.
- Engine verdicts cover SQL Server 2017–2025 at compatibility levels 100–170 (2008–2016 are not verified),
  QUOTED_IDENTIFIER ON and OFF; other session settings (ANSI_NULLS and the rest) were left at the defaults.
- Errors that need context (undeclared variables, conflicting hints, boolean versus scalar, names, permissions)
  are outside a grammar; some Azure Synapse syntax kept from 1.0.0 is accepted by the grammar but not by SQL Server.
- The WASM module is the unoptimised recipe (no Binaryen); an optimised build changes size and speed. The native and
  WASM tree-check runs were not alternated.
- 2.0.0's slowdown on clean input (about 13 %) is attributed to the larger tables, Unicode identifier sets and extra
  reductions from an earlier counter study; its parts were not separated here. The [2.1.0](#210) study re-took the
  counters, and splits its remaining gap only by input bucket.
- Incremental exactness was measured on 8,063 edits of four simple kinds with tree-sitter 0.27.2; tree-sitter 0.27.3
  was not tested. The re-lex figures come from a separate study (counters, not timings).
- 1.0.0 was measured with the same benchmark program and runtime, not with its own CI or tooling.
