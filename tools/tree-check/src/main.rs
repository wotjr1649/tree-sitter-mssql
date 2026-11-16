//! Checks each T-SQL file's parse tree through the tree-sitter API and prints one JSON line per file.
//!
//! A tree passes when its root does not report `has_error` and it has no `ERROR` node, no `MISSING` node, no zero-width node, and its root
//! spans the whole input, and it breaks none of the option-set constraints of ADR-0003
//! (`constraints.rs`), which SQL Server enforces while parsing. The CLI's parse summary is not used: it can report success for trees that
//! contain `ERROR` or only zero-width nodes.
//!
//! Usage: `tree-check [--quoted-identifier on|off] [--keyword-ranges] [--paths LIST] [FILE...]`. Exit code 0: every tree passes; 1: some tree has a
//! problem; 2: a file could not be read or parsed, so nothing is judged.
//!
//! `tree-check --edits HISTORIES.jsonl` instead replays edit histories and compares every
//! incremental reparse with a fresh parse (see `edits.rs`). Exit code 1: some step differs.

use std::process::ExitCode;
use std::{env, fs};
use tree_check::{
    QuotedIdentifier, edits, grammar_parser, inspect_with_quoted_identifier, json_line,
    json_string, parse,
};

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

fn keyword_ranges(tree: &tree_sitter::Tree, source: &[u8]) -> Vec<[usize; 2]> {
    let mut ranges = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.child_count() == 0
            && !node.is_extra()
            && !node.is_missing()
            && std::iter::successors(Some(node), |n| n.parent())
                .any(|n| n.kind().starts_with("keyword_"))
        {
            ranges.push([node.start_byte(), node.end_byte()]);
        }
        if node.kind().starts_with("keyword_") && node.child_count() > 0 {
            // Hidden regex terminals have no child node; retain their text between visible children.
            let mut start = node.start_byte();
            let mut children = node.walk();
            for end in node
                .children(&mut children)
                .map(|n| (n.start_byte(), n.end_byte()))
                .chain(std::iter::once((node.end_byte(), node.end_byte())))
            {
                let mut at = start;
                while at < end.0 {
                    if source[at].is_ascii_whitespace() {
                        at += 1;
                        continue;
                    }
                    let token = at;
                    while at < end.0 && !source[at].is_ascii_whitespace() {
                        at += 1;
                    }
                    ranges.push([token, at]);
                }
                start = end.1;
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                ranges.sort_unstable();
                ranges.dedup();
                return ranges;
            }
        }
    }
}

fn token_ranges(tree: &tree_sitter::Tree) -> Vec<[usize; 2]> {
    let mut ranges = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.child_count() == 0
            && !node.is_missing()
            && !node.is_error()
            && node.start_byte() < node.end_byte()
        {
            ranges.push([node.start_byte(), node.end_byte()]);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return ranges;
            }
        }
    }
}

fn run(args: &[String]) -> Result<bool, String> {
    if args.len() == 2 && args[0] == "--edits" {
        return run_edits(&args[1]);
    }
    let mut paths = Vec::new();
    let mut include_keyword_ranges = false;
    let mut quoted_identifier = QuotedIdentifier::On;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--quoted-identifier" {
            quoted_identifier = match it.next().map(String::as_str) {
                Some("on") => QuotedIdentifier::On,
                Some("off") => QuotedIdentifier::Off,
                _ => return Err("--quoted-identifier needs on or off".into()),
            };
        } else if arg == "--keyword-ranges" {
            include_keyword_ranges = true;
        } else if arg == "--paths" {
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
            "usage: tree-check [--quoted-identifier on|off] [--keyword-ranges] [--paths LIST] [FILE...] | tree-check --edits HISTORIES".into(),
        );
    }
    let mut parser = grammar_parser()?;
    let mut all_ok = true;
    for path in &paths {
        let source = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let tree = parse(&mut parser, &source).map_err(|e| format!("{path}: {e}"))?;
        let report = inspect_with_quoted_identifier(&tree, &source, quoted_identifier);
        all_ok &= report.ok();
        let mut line = json_line(path, &report);
        if include_keyword_ranges {
            let ranges = keyword_ranges(&tree, &source);
            line.pop();
            line.push_str(&format!(
                ",\"keyword_ranges\":{},\"token_ranges\":{}}}",
                serde_json::to_string(&ranges).unwrap(),
                serde_json::to_string(&token_ranges(&tree)).unwrap()
            ));
        }
        println!("{line}");
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

    #[test]
    fn keyword_ranges_exclude_nested_comments_and_quoted_tokens() {
        let sql = "CREATE TABLE [한글] (c CHARACTER /* comment select */ VARYING(5)); SELECT 'from', \"where\";";
        let mut parser = grammar_parser().unwrap();
        let tree = parse(&mut parser, sql.as_bytes()).unwrap();
        assert!(!tree.root_node().has_error());
        let words: Vec<_> = keyword_ranges(&tree, sql.as_bytes())
            .iter()
            .map(|[start, end]| &sql[*start..*end])
            .collect();
        assert_eq!(words, ["CREATE", "TABLE", "CHARACTER", "VARYING", "SELECT"]);
    }

    #[test]
    fn token_ranges_preserve_scanner_operators_and_atomic_multiword_keywords() {
        let sql = "SELECT * FROM dbo.t FOR SYSTEM_TIME ALL WHERE a ! /* 기밀 */ = 2;";
        let mut parser = grammar_parser().unwrap();
        let tree = parse(&mut parser, sql.as_bytes()).unwrap();
        assert!(!tree.root_node().has_error());
        let tokens: Vec<_> = token_ranges(&tree)
            .iter()
            .map(|[start, end]| &sql[*start..*end])
            .collect();
        assert!(tokens.contains(&"FOR SYSTEM_TIME"), "{tokens:?}");
        assert!(tokens.contains(&"! /* 기밀 */ ="), "{tokens:?}");
    }

    #[test]
    fn quoted_identifier_rejects_unknown_or_missing_values() {
        for args in [
            vec!["--quoted-identifier"],
            vec!["--quoted-identifier", "maybe"],
        ] {
            assert!(
                run(&args.into_iter().map(String::from).collect::<Vec<_>>())
                    .unwrap_err()
                    .contains("on or off")
            );
        }
    }
}
