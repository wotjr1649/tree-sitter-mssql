#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C LD_BIND_NOW=1

# Keep inputs fixed across parser revisions and workflow runs.
inputs_commit=0c7d477607e9d70c7d662bc1f8fa9fb910f841fc # v2.2.0
runtime_version=0.27.2
runtime_commit=3fb7be49145acfb07e33b61ed5689e2b6771b2e9 # tree-sitter v0.27.2
runtime_sha256=fc5fd4dbca1149231c8a9d87a39fbb9ead00cb21bbfc60778b1c8d727f732945

bench_refs=${BENCH_REFS:-v2.1.1 v2.2.0 HEAD}
max_increase=${BENCH_MAX_INCREASE_PCT:-}
if [[ ! "$bench_refs" =~ ^[a-zA-Z0-9_./\ -]+$ || ! "$bench_refs" =~ [a-zA-Z0-9_./-] ]]; then
  echo 'bench_refs must be space-separated Git refs or commit hashes' >&2
  exit 2
fi
read -r -a refs <<< "$bench_refs"
if (( ${#refs[@]} < 2 )); then
  echo 'bench_refs must contain a baseline and at least one comparison ref' >&2
  exit 2
fi
if [[ -n "$max_increase" && ! "$max_increase" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
  echo 'bench_max_increase_pct must be a nonnegative decimal percentage' >&2
  exit 2
fi
if ! grep -Fxq "tree-sitter = \"=$runtime_version\"" crates/tree-check/Cargo.toml; then
  echo 'The benchmark runtime must match the tree-check runtime pin' >&2
  exit 2
fi

mkdir -p build/parse-bench
work=$(mktemp -d "$PWD/build/parse-bench/run.XXXXXX")
artifacts="$work/artifacts"
mkdir -p "$artifacts" "$work/inputs"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  printf 'artifacts=%s\n' "$artifacts" >> "$GITHUB_OUTPUT"
fi

for ref in "${refs[@]}"; do
  commit=$(git rev-parse --quiet --verify --end-of-options "$ref^{commit}" ||
    git rev-parse --verify --end-of-options "refs/remotes/origin/$ref^{commit}")
  printf '%s\t%s\n' "$ref" "$commit"
done > "$artifacts/refs.tsv"

git archive "$inputs_commit" test/fixtures test/corpus oracle/data/kit/cases.jsonl oracle/data/probes |
  tar -x -C "$work/inputs"
bench="$PWD/scripts/parse-bench.mjs"
(cd "$work/inputs" && node "$bench" pack "$work" "$artifacts")

flags=(-std=gnu11 -O2 -g -DNDEBUG)
{
  echo "Input commit: $inputs_commit"
  echo "tree-sitter: $runtime_version ($runtime_commit)"
  echo "Runtime archive SHA-256: $runtime_sha256"
  echo "CFLAGS: ${flags[*]}"
  echo "Runner image: ${ImageOS:-unknown} ${ImageVersion:-unknown}"
  uname -sm
  gcc --version
  ldd --version
  valgrind --version
  node --version
} > "$artifacts/tools.txt"

archive="$work/tree-sitter.tar.gz"
runtime="$work/tree-sitter-$runtime_commit/lib"
curl --fail --silent --show-error --location \
  "https://codeload.github.com/tree-sitter/tree-sitter/tar.gz/$runtime_commit" -o "$archive"
printf '%s  %s\n' "$runtime_sha256" "$archive" | sha256sum --check
tar -xzf "$archive" -C "$work" "tree-sitter-$runtime_commit/lib"
gcc "${flags[@]}" -I"$runtime/include" -I"$runtime/src" -c "$runtime/src/lib.c" -o "$work/runtime.o"
gcc "${flags[@]}" -DPARSE_BENCH_CALLGRIND -I"$runtime/include" \
  -c scripts/parse-bench.c -o "$work/harness.o"

i=0
while IFS=$'\t' read -r ref commit; do
  grammar="$work/grammar-$i"
  mkdir -p "$grammar"
  git archive "$commit" src/parser.c src/scanner.c src/tree_sitter | tar -x -C "$grammar"
  (cd "$grammar" && sha256sum src/parser.c src/scanner.c src/tree_sitter/*.h) > "$artifacts/$i-sources.sha256"
  gcc "${flags[@]}" -I"$grammar/src" "$grammar/src/parser.c" "$grammar/src/scanner.c" \
    "$work/runtime.o" "$work/harness.o" -o "$work/parse-$i"
  for set in fixtures corpus oracle; do
    for mode in fresh incremental; do
      out="$artifacts/$i-$set-$mode"
      echo "Measuring $ref $set $mode"
      valgrind --tool=callgrind --collect-atstart=no --dump-line=no \
        --callgrind-out-file="$out.callgrind" "$work/parse-$i" "$work/$set.bin" "$mode" > "$out.tally"
    done
  done
  i=$((i + 1))
done < "$artifacts/refs.tsv"
node "$bench" summary "$artifacts" "$max_increase"
