# CGO 없는 호스트를 위한 WASM 제약 검사기

네이티브 `tree-check` CLI와 WASI Preview 1 reactor는 같은 Rust 라이브러리의 문법·제약
규칙을 사용한다. Go 소비자는 규칙을 Go로 이식하거나 CGO를 사용하지 않고 이 모듈을
호출할 수 있다. 모듈에는 문법과 tree-sitter 0.27.2 런타임이 포함되므로, 외부 런타임의
트리가 아닌 SQL 소스 바이트를 입력한다.

## 고정 빌드와 identity

전체 Git 이력이 있는 checkout, Rust **1.100.0**의 `wasm32-wasip1` target,
압축 파일 SHA-256을 검증한 **wasi-sdk 34.0**을 사용한다. SDK URL·체크섬은
[`wasm-pins.json`](../tools/tree-check/wasm-pins.json)에 있다. Linux SDK는
go-treesitter와 같은 pin이며 Verify에서 압축 해제 전에 체크섬을 검증하고 캐시한다.
빌드 스크립트는 도구를 다운로드하지 않는다. Windows에서는 GNU Rust 호스트 툴체인을 사용한다.

```sh
export WASI_SDK_PATH=/absolute/path/to/wasi-sdk-34.0-x86_64-linux
export CARGO_TARGET_DIR=target
node tools/tree-check-wasm-build.mjs
cargo build --locked --manifest-path tools/tree-check/Cargo.toml --bin tree-check
node tools/tree-check-wasm-parity.mjs \
  --wasm target/wasm32-wasip1/release/tree_check.reactor.wasm \
  --tree-check target/debug/tree-check
```

C/Rust 최적화는 `-O3`/`opt-level=3`, fat LTO, codegen unit 1개, panic abort,
symbol 제거, debug 정보·incremental compilation 비활성화로 고정한다. 소스와 캐시의
로컬 절대 경로도 remap한다. 스크립트는 모듈 크기·SHA-256·빌드 시간·실제 import와
export를 출력한다. 기본 recipe는 Binaryen을 사용하지 않는다. `WASM_OPT`를 명시하면
**version 133**만 허용하며 별도의 Binaryen `-O3` recipe를 선택한다. 다른 버전을
조용히 대체하지 않는다. recipe나 호스트 툴체인이 다른 빌드의 hash 동일성을 가정하지 말고,
같은 고정 입력으로 재빌드해 확인한다.

`tc_identity()`는 다음 JSON을 담은 소유권 있는 결과 핸들을 반환한다.

```json
{"abi_version":1,"grammar_commit":"<40 hex>","rules_sha256":"<64 hex>"}
```

`grammar_commit`은 문법 소스·생성 parser·scanner를 마지막으로 변경한 commit이다.
스크립트는 미커밋 문법 변경과 shallow clone을 거부한다. `rules_sha256`은
`tools/tree-check/src/constraints.rs`, 같은 디렉터리의 `lib.rs`, `wasm.rs` 순서로
각 파일의 전체 상대 경로, NUL, CRLF를 LF로 정규화한 UTF-8 내용, NUL을 이어 붙인
SHA-256이다. 아직 커밋하지 않은 규칙 변경도 식별한다. 저장소 commit과 모듈 SHA-256을
함께 보관해야 한다. identity 자체는 artifact의 서명이나 인증 수단이 아니다.

## ABI version 1

`memory`와 아래 함수를 export한다. wasm32에서 모든 인수와 반환값은 **부호 없는 값으로
해석하는 `i32`**이며, 포인터는 해당 인스턴스 linear memory의 바이트 offset이다.
C 문자열·NUL 종결자·공유 메모리·callback·호스트 메모리 포인터는 사용하지 않는다.
인스턴스를 만든 뒤 `_initialize`가 있으면 한 번 호출한다. `_start`는 없다.

| Export | 인수 | 반환값 / 동작 |
| --- | --- | --- |
| `tc_alloc` | `len` | 쓰기 가능한 입력 포인터. 할당 실패 시 `0`. 길이 0도 지원한다. |
| `tc_free` | `ptr, len` | 원래 할당 길이와 일치하는 입력 버퍼를 해제한다. 잘못된 조합은 무시한다. |
| `tc_check` | `source_ptr, source_len` | JSON 결과 핸들. 잘못된 입력이나 내부 실패 시 `0`. |
| `tc_result_ptr` | `handle` | 읽기 전용 JSON 바이트 offset. 없거나 해제된 핸들은 `0`. |
| `tc_result_len` | `handle` | JSON 바이트 길이. 없거나 해제된 핸들은 `0`. |
| `tc_result_free` | `handle` | 결과를 해제한다. 없거나 해제된 핸들은 무시한다. |
| `tc_identity` | 없음 | 소유권 있는 identity JSON 결과 핸들. |

버퍼를 할당하고 소스 바이트를 복사한 뒤 `tc_check`를 호출하고 `tc_free`로 입력을 해제한다.
입력 포인터는 아직 해제하지 않은 `tc_alloc`의 시작 주소여야 하며 입력 길이는 원래 요청한
할당 길이를 초과할 수 없다. 유효하거나 잘못된 UTF-8 바이트 모두 parser 입력으로 받는다.
SQL 거절은 `ok:false`를 담은 0이 아닌 핸들이다. 핸들 `0`은 SQL 판정이 아닌 API 실패다.

결과 포인터와 길이를 조회해 JSON을 복사·해석한 뒤 `tc_result_free`로 해제한다.
입력을 해제하거나 다음 검사를 실행해도 결과는 유지되며 여러 결과를 동시에 보관할 수 있다.
모듈은 살아 있는 결과를 수정하지 않는다. 결과 핸들은 재사용 없이 단조 증가하므로,
결과 `2^32-1`개를 생성한 이후에는 새 인스턴스를 만든다. 결과 메모리를 수정하거나 해제한
입력 포인터를 사용해서는 안 된다. allocator는 해제한 입력의 offset을 재사용할 수 있다.

각 검사는 새 parser와 tree를 만들고 반환 전에 해제한다. 한 인스턴스의 호출은 직렬화해야
하며 재진입·스레드 안전성을 제공하지 않는다. 별도 인스턴스는 독립적으로 병렬 실행할 수 있다.
메모리는 할당·검사 중 증가할 수 있으므로, 모듈 호출 이후 입력 복사나 결과 읽기 전에 새
메모리 view를 얻는다. 버퍼 해제는 메모리 재사용을 허용하지만 linear memory를 줄이지 않는다.
호스트는 처리할 SQL에 맞는 메모리·실행 예산을 정해야 한다. OOM·panic·스택 한계는 trap을
발생시킬 수 있다. trap 이후에는 allocator 상태를 가정하지 말고 인스턴스를 폐기한다.

## 입력별 JSON

WASM은 CLI와 같은 입력별 JSON을 반환하되 파일 이름은 호출자가 관리하므로 `path:""`다.
모든 개수·offset은 음이 아닌 정수다. `row`와 `column`은 0부터 시작하며, column과 범위는
Unicode 문자 수가 아닌 UTF-8 **바이트** 단위다.

```json
{"path":"","ok":true,"root_has_error":false,"error_nodes":0,"missing_nodes":0,"zero_width_nodes":0,"root_end":9,"len":9,"first_problem":null,"constraint_violations":[]}
```

| 필드 | 의미 |
| --- | --- |
| `path` | CLI 입력 경로. WASM에서는 항상 빈 문자열. |
| `ok` | 아래 검사 조건을 모두 통과했는지 여부. |
| `root_has_error` | tree-sitter root의 recovery/error flag. |
| `error_nodes` | `ERROR` node 수. |
| `missing_nodes` | `MISSING` node 수. |
| `zero_width_nodes` | 입력이 비어 있지 않을 때 missing이 아닌 zero-width node 수. |
| `root_end`, `len` | Root 종료 바이트와 입력 바이트 길이. `ok`이려면 같아야 한다. |
| `first_problem` | `null` 또는 문서 순서상 첫 트리 문제의 `{kind:string,row:integer,column:integer}`. |
| `constraint_violations` | 순서를 보존한 `{rule:string,detail:string,row:integer,column:integer}` 배열. 하나라도 있으면 `ok:false`. |

`first_problem`은 트리 문제만 다루므로 제약만 위반한 경우 `null`일 수 있다.
빈 소스에도 CLI와 같은 판정 규칙을 적용한다. `ok`은 batch 실행 가능성이나 이름·권한·타입의
정확성을 보장하지 않는다.

## 엔진 일치와 parity gate

문법과 제약 계층을 함께 사용한 판정은 커밋된 SQL Server oracle에서 엔진이 허용한 모든
사례를 통과시켜야 하며, 엔진이 거절한 사례는
[`oracle/rejected-clean.json`](../oracle/rejected-clean.json)의 **명시적으로 분류한 예외**를
제외하고 거절해야 한다. 예외에는 트리 제약의 gap, context, 엔진 version, environment,
runner 한계가 있다. 이는 기록된 oracle·호환성 수준에서 측정한 보장이며 가능한 모든
T-SQL에 대한 보장이 아니다. 정확한 baseline은 `oracle/expected.json`과 엔진 판정을 참조한다.
WASM 도입은 그 baseline을 갱신하지 않고 보존한다.

Verify는 기존 네이티브 oracle replay로 증분 편집과 예외 분류를 검사하고,
`oracle/kit/cases.jsonl`, `oracle/probes/*.jsonl`, `test/fixtures/*.sql`, 각 편집 이력의
초기·중간 소스와 ABI 경계 사례 전체에서 WASM/네이티브 JSON의 모든 필드를 비교한다.
호출자가 관리하는 경로만 다르다. 사례 누락·추가, 결과 누락, JSON 차이는 모두 실패다.
잘못된 포인터·핸들, 결과 동시 보관, 빈 입력·Unicode·잘못된 UTF-8과 memory growth도 검사한다.
CI summary에 batch 시간과 import 이름을 기록한다. 네이티브 시간에는 프로세스 시작·파일
읽기가, WASM 시간에는 할당·판정·JSON 해석이 포함된다. 두 수치는 순수 parser
microbenchmark가 아닌 각 통합 경로의 측정값이다.

## CGO 없는 Go 호스트 연결

**wazero**에서는 WASI Preview 1과 이 reactor를 인스턴스화한다. 검사기에 filesystem
preopen·인수·환경변수를 제공할 필요는 없다. `_start`를 실행하지 않도록 reactor 초기화를
설정하고 exported memory와 함수를 위 소유권 순서대로 호출한다. `i32` 값은 `uint32`로
다루며 핸들을 해제하기 전에 JSON을 복사한다. worker마다 별도 모듈 인스턴스를 두고 직렬 호출한다.

**wasm2go**에서는 go-treesitter의 고정 translation pipeline으로 같은 reactor를 변환하고
남아 있는 WASI import를 제공한다. 변환 결과는 growable linear memory, export 함수와
reactor 초기화를 보존해야 한다. 이 모듈에는 Rust·제약 규칙·tree-sitter C 런타임·allocator가
포함되므로 문법 전용 WASM loader만으로 대체할 수 없다. 변환한 Go 호스트에서도
`tools/tree-check-wasm-parity.mjs`와 같은 입력과 JSON baseline으로 비교한다.
이 변경은 Go 쪽 연결을 구현하거나 검증하지 않는다.

## 남은 WASI import

기본 고정 빌드는 다음 **9개 함수만** `wasi_snapshot_preview1`에서 import한다.
다른 module·memory·table·global import는 없다. 이는 연결된 WASI libc/Rust 런타임의
호출 경로를 포함한다. 규칙의 두 중복 검사 집합을 `BTreeSet`으로 바꿔 난수 seed와
`random_get` 의존성을 제거했다. JSON 판정과 위반 배열 순서는 그대로다.

| 함수 | WASI 기능 |
| --- | --- |
| `environ_get` | 환경변수 문자열 조회 |
| `environ_sizes_get` | 환경변수 개수·버퍼 크기 조회 |
| `clock_time_get` | 런타임 시간 조회 |
| `fd_close` | 파일 descriptor 닫기 |
| `fd_prestat_get` | preopen descriptor 정보 조회 |
| `fd_prestat_dir_name` | preopen 디렉터리 이름 조회 |
| `fd_seek` | descriptor 위치 변경 |
| `fd_write` | 출력 descriptor 쓰기; 런타임 진단 경로 포함 |
| `proc_exit` | 프로세스 종료 요청; 정상 SQL 판정 반환 경로가 아님 |

Node WASI에서 import를 감싸 호출 수를 측정했을 때 `_initialize`, identity 조회와
전체 12,009개 입력의 정상 반환 과정에서 **9개 함수 모두 호출 0회**였다. 이는 이 입력
집합에서의 관찰이며 trap·panic·런타임 실패 시에도 호출되지 않는다는 약속은 아니다.
호스트는 이 import를 연결해야 하며 정상 검사에는 파일 권한·preopen이 필요하지 않다.
