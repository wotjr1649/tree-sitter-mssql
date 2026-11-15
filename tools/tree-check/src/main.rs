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

mod constraints;
mod edits;

use std::fmt::Write as _;
use std::process::ExitCode;
use std::{env, fs};
use tree_sitter::{Parser, Tree};

#[derive(Debug, Default, PartialEq)]
struct Report {
    /// `has_error` of the root node; recovery can set it without leaving an `ERROR` node.
    root_has_error: bool,
    error_nodes: usize,
    missing_nodes: usize,
    zero_width_nodes: usize,
    root_end: usize,
    len: usize,
    /// Kind, row and column (0-based) of the first problem node in document order.
    first_problem: Option<(String, usize, usize)>,
    /// Option-set constraint violations (ADR-0003).
    violations: Vec<constraints::Violation>,
}

impl Report {
    fn ok(&self) -> bool {
        !self.root_has_error
            && self.error_nodes == 0
            && self.missing_nodes == 0
            && self.zero_width_nodes == 0
            && self.root_end == self.len
            && self.violations.is_empty()
    }
}

fn parse(parser: &mut Parser, source: &[u8]) -> Result<Tree, String> {
    parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter returned no tree".to_string())
}

fn inspect(tree: &Tree, source: &[u8]) -> Report {
    let len = source.len();
    let root = tree.root_node();
    let mut report = Report {
        root_has_error: root.has_error(),
        root_end: root.end_byte(),
        len,
        violations: constraints::check(tree, source),
        ..Report::default()
    };
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        let error = node.is_error();
        let missing = node.is_missing();
        // A missing node is zero-width by construction; count it once, as missing.
        let zero_width = !missing && node.start_byte() == node.end_byte() && len > 0;
        report.error_nodes += usize::from(error);
        report.missing_nodes += usize::from(missing);
        report.zero_width_nodes += usize::from(zero_width);
        if (error || missing || zero_width) && report.first_problem.is_none() {
            let at = node.start_position();
            let kind = if missing {
                format!("MISSING {}", node.kind())
            } else {
                node.kind().into()
            };
            report.first_problem = Some((kind, at.row, at.column));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return report;
            }
        }
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_line(path: &str, r: &Report) -> String {
    let first = match &r.first_problem {
        Some((kind, row, column)) => {
            format!(
                r#"{{"kind":{},"row":{row},"column":{column}}}"#,
                json_string(kind)
            )
        }
        None => "null".into(),
    };
    let violations: Vec<String> = r
        .violations
        .iter()
        .map(|v| {
            format!(
                r#"{{"rule":{},"detail":{},"row":{},"column":{}}}"#,
                json_string(v.rule),
                json_string(&v.detail),
                v.row,
                v.column
            )
        })
        .collect();
    format!(
        r#"{{"path":{},"ok":{},"root_has_error":{},"error_nodes":{},"missing_nodes":{},"zero_width_nodes":{},"root_end":{},"len":{},"first_problem":{first},"constraint_violations":[{}]}}"#,
        json_string(path),
        r.ok(),
        r.root_has_error,
        r.error_nodes,
        r.missing_nodes,
        r.zero_width_nodes,
        r.root_end,
        r.len,
        violations.join(",")
    )
}

fn grammar_parser() -> Result<Parser, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_mssql::LANGUAGE.into())
        .map_err(|e| format!("cannot load the T-SQL grammar: {e}"))?;
    Ok(parser)
}

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

    fn check(sql: &str) -> Report {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_mssql::LANGUAGE.into())
            .unwrap();
        let tree = parse(&mut parser, sql.as_bytes()).unwrap();
        inspect(&tree, sql.as_bytes())
    }

    #[test]
    fn clean_statement_passes() {
        let r = check("SELECT a FROM dbo.t WHERE a > 1;\n");
        assert!(r.ok(), "{r:?}");
        assert_eq!(r.first_problem, None);
    }

    #[test]
    fn error_node_fails() {
        let r = check("SELECT * FROM dbo.t WITH (FORCESEEK(-1(a)));\n");
        assert!(r.error_nodes > 0, "{r:?}");
        assert!(!r.ok());
    }

    #[test]
    fn zero_width_node_fails() {
        // The CLI text dump shows no ERROR or MISSING here, but has zero-width nodes.
        let r = check("CREATE ASSEMBLY a FROM 'x' WITH x ();\n");
        assert!(r.zero_width_nodes > 0, "{r:?}");
        assert!(!r.ok());
    }

    #[test]
    fn missing_node_fails_and_is_reported_first() {
        let r = check("SELECT (1;\n");
        assert!(!r.ok(), "{r:?}");
        let (kind, row, _) = r.first_problem.expect("a problem");
        assert!(kind.starts_with("MISSING") || kind == "ERROR", "{kind}");
        assert_eq!(row, 0);
    }

    #[test]
    fn unterminated_comment_fails() {
        assert!(!check("SELECT 1; /* open\n").ok());
    }

    #[test]
    fn json_escapes_paths_and_kinds() {
        let r = Report {
            len: 1,
            root_end: 1,
            ..Report::default()
        };
        let line = json_line("C:\\t\"q\".sql", &r);
        assert!(
            line.starts_with(r#"{"path":"C:\\t\"q\".sql","ok":true"#),
            "{line}"
        );
        assert_eq!(json_string("a\nb"), r#""a\u000ab""#);
    }

    #[test]
    fn unreadable_file_stops_without_a_verdict() {
        let err = run(&["no/such/file.sql".to_string()]).unwrap_err();
        assert!(err.contains("no/such/file.sql"), "{err}");
    }
}
