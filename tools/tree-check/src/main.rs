//! Checks each T-SQL file's parse tree through the tree-sitter API and prints one JSON line per file.
//!
//! A tree passes when its root does not report `has_error` and it has no `ERROR` node, no `MISSING` node, no zero-width node, and its root
//! spans the whole input, and it breaks none of the option-set constraints of ADR-0003
//! (`constraints.rs`), which SQL Server enforces while parsing. The CLI's parse summary is not used: it can report success for trees that
//! contain `ERROR` or only zero-width nodes.
//!
//! Usage: `tree-check [--paths LIST] [FILE...]`. Exit code 0: every tree passes; 1: some tree has a
//! problem; 2: a file could not be read or parsed, so nothing is judged.
//!
//! `tree-check --edits HISTORIES.jsonl` instead replays edit histories and compares every
//! incremental reparse with a fresh parse (see `edits.rs`). Exit code 1: some step differs.

use std::process::ExitCode;
use std::{env, fs};
use tree_check::{edits, grammar_parser, inspect, json_line, json_string, parse};

fn run_edits(file: &str) -> Result<bool, String> {
    let text = fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let histories = edits::read_histories(&text).map_err(|e| format!("{file}: {e}"))?;
    let mut parser = grammar_parser()?;
    let mut all_equal = true;
    for h in &histories {
        let differing = edits::check_history(&mut parser, h)?;
        all_equal &= differing.is_empty();
        let steps: Vec<String> = differing.iter().map(usize::to_string).collect();
        println!(
            r#"{{"id":{},"steps":{},"incremental_differences":[{}]}}"#,
            json_string(&h.id),
            h.edits.len(),
            steps.join(",")
        );
    }
    Ok(all_equal)
}

fn run(args: &[String]) -> Result<bool, String> {
    if args.len() == 2 && args[0] == "--edits" {
        return run_edits(&args[1]);
    }
    let mut paths = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--paths" {
            let list = it.next().ok_or("--paths needs a file")?;
            let text = fs::read_to_string(list).map_err(|e| format!("{list}: {e}"))?;
            paths.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from),
            );
        } else {
            paths.push(arg.clone());
        }
    }
    if paths.is_empty() {
        return Err(
            "usage: tree-check [--paths LIST] [FILE...] | tree-check --edits HISTORIES".into(),
        );
    }
    let mut parser = grammar_parser()?;
    let mut all_ok = true;
    for path in &paths {
        let source = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let tree = parse(&mut parser, &source).map_err(|e| format!("{path}: {e}"))?;
        let report = inspect(&tree, &source);
        all_ok &= report.ok();
        println!("{}", json_line(path, &report));
    }
    Ok(all_ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(message) => {
            eprintln!("tree-check: {message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_file_stops_without_a_verdict() {
        let err = run(&["no/such/file.sql".to_string()]).unwrap_err();
        assert!(err.contains("no/such/file.sql"), "{err}");
    }
}
