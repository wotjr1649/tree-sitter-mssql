//! Version 1 reactor ABI. All pointers refer to this instance's linear memory.

use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Default)]
struct State {
    inputs: BTreeMap<usize, (usize, Box<[u8]>)>,
    results: BTreeMap<u32, Box<[u8]>>,
    next_result: u32,
}

impl State {
    fn result(&mut self, json: String) -> u32 {
        let Some(id) = self.next_result.checked_add(1) else {
            return 0;
        };
        self.next_result = id;
        self.results
            .insert(id, json.into_bytes().into_boxed_slice());
        id
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Allocate a writable input buffer. Zero means allocation failed.
#[unsafe(no_mangle)]
pub extern "C" fn tc_alloc(len: usize) -> usize {
    let mut bytes = Vec::new();
    if bytes.try_reserve_exact(len.max(1)).is_err() {
        return 0;
    }
    bytes.resize(len.max(1), 0);
    let bytes = bytes.into_boxed_slice();
    let ptr = bytes.as_ptr() as usize;
    STATE.with_borrow_mut(|state| state.inputs.insert(ptr, (len, bytes)));
    ptr
}

/// Release an input buffer; a wrong pointer or allocation length is ignored.
#[unsafe(no_mangle)]
pub extern "C" fn tc_free(ptr: usize, len: usize) {
    STATE.with_borrow_mut(|state| {
        if state.inputs.get(&ptr).is_some_and(|(size, _)| *size == len) {
            state.inputs.remove(&ptr);
        }
    });
}

/// Inspect bytes in a live tc_alloc buffer. Zero means an invalid input or internal failure.
#[unsafe(no_mangle)]
pub extern "C" fn tc_check(ptr: usize, len: usize) -> u32 {
    STATE.with_borrow_mut(|state| {
        let Some((size, bytes)) = state.inputs.get(&ptr) else {
            return 0;
        };
        if len > *size {
            return 0;
        }
        let Ok(mut parser) = crate::grammar_parser() else {
            return 0;
        };
        let Ok(tree) = crate::parse(&mut parser, &bytes[..len]) else {
            return 0;
        };
        let report = crate::inspect(&tree, &bytes[..len]);
        state.result(crate::json_line("", &report))
    })
}

/// Return an immutable UTF-8 JSON pointer; valid until tc_result_free(handle).
#[unsafe(no_mangle)]
pub extern "C" fn tc_result_ptr(handle: u32) -> usize {
    STATE.with_borrow(|state| {
        state
            .results
            .get(&handle)
            .map_or(0, |bytes| bytes.as_ptr() as usize)
    })
}

/// Return the JSON byte length, excluding any terminator (there is no NUL terminator).
#[unsafe(no_mangle)]
pub extern "C" fn tc_result_len(handle: u32) -> usize {
    STATE.with_borrow(|state| state.results.get(&handle).map_or(0, |bytes| bytes.len()))
}

/// Release a result. Unknown and already released handles are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn tc_result_free(handle: u32) {
    STATE.with_borrow_mut(|state| state.results.remove(&handle));
}

/// Return an owned identity JSON result, released with tc_result_free.
#[unsafe(no_mangle)]
pub extern "C" fn tc_identity() -> u32 {
    #[cfg(target_arch = "wasm32")]
    let (grammar, rules) = (
        env!(
            "TREE_CHECK_GRAMMAR_COMMIT",
            "build with tools/tree-check-wasm-build.mjs"
        ),
        env!(
            "TREE_CHECK_RULES_SHA256",
            "build with tools/tree-check-wasm-build.mjs"
        ),
    );
    #[cfg(not(target_arch = "wasm32"))]
    let (grammar, rules) = ("native-test", "native-test");
    STATE.with_borrow_mut(|state| {
        state.result(
            serde_json::json!({
                "abi_version": 1,
                "grammar_commit": grammar,
                "rules_sha256": rules,
            })
            .to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_and_invalid_arguments() {
        assert_eq!(tc_check(0, 0), 0);
        assert_eq!(tc_result_ptr(0), 0);
        assert_eq!(tc_result_len(0), 0);
        tc_free(0, 0);
        tc_result_free(0);
        assert_eq!(tc_alloc(usize::MAX), 0);
        let ptr = tc_alloc(0);
        assert_ne!(ptr, 0);
        assert_eq!(tc_check(ptr, 1), 0);
        tc_free(ptr, 1);
        let result = tc_check(ptr, 0);
        assert_ne!(result, 0);
        let second = tc_check(ptr, 0);
        assert_ne!(result, second);
        tc_free(ptr, 0);
        assert_eq!(tc_check(ptr, 0), 0);
        STATE.with_borrow(|state| {
            let json: serde_json::Value = serde_json::from_slice(&state.results[&result]).unwrap();
            assert_eq!(json["len"], 0);
            assert_eq!(json["constraint_violations"], serde_json::json!([]));
        });
        tc_result_free(result);
        assert_eq!(tc_result_ptr(result), 0);
        assert_eq!(tc_result_len(result), 0);
        assert_ne!(tc_result_ptr(second), 0);
        tc_result_free(result);
        tc_result_free(second);
        let identity = tc_identity();
        STATE.with_borrow(|state| {
            let json: serde_json::Value =
                serde_json::from_slice(&state.results[&identity]).unwrap();
            assert_eq!(json["abi_version"], 1);
        });
        tc_result_free(identity);
        STATE.with_borrow(|state| {
            assert!(state.inputs.is_empty());
            assert!(state.results.is_empty());
        });
    }
}
