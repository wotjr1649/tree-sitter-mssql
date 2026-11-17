#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| tree_sitter_mssql_fuzz::check(data));
