//! T-SQL tree inspection and constraint rules shared by the CLI and WASM ABI.

mod constraints;
pub mod edits;
mod session;
#[doc(hidden)]
pub mod tree;
#[cfg(any(target_arch = "wasm32", test))]
mod wasm;

use std::fmt::Write as _;
use tree_sitter::{Parser, Tree};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuotedIdentifier {
    #[default]
    On,
    Off,
}

#[derive(Debug, Default, PartialEq)]
pub struct Report {
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
    pub fn ok(&self) -> bool {
        !self.root_has_error
            && self.error_nodes == 0
            && self.missing_nodes == 0
            && self.zero_width_nodes == 0
            && self.root_end == self.len
            && self.violations.is_empty()
    }
}

pub fn parse(parser: &mut Parser, source: &[u8]) -> Result<Tree, String> {
    parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter returned no tree".to_string())
}

pub fn inspect(tree: &Tree, source: &[u8]) -> Report {
    inspect_with_quoted_identifier(tree, source, QuotedIdentifier::On)
}

pub fn inspect_with_quoted_identifier(
    tree: &Tree,
    source: &[u8],
    quoted_identifier: QuotedIdentifier,
) -> Report {
    if !source.contains(&b'"') {
        return inspect_tree(tree, source);
    }
    let (projected, origins) =
        session::project(source, quoted_identifier, &session::module_ranges(tree));
    if projected == source {
        return inspect_tree(tree, source);
    }
    // The public tree is untouched; only the checker parses the session's string spelling.
    let mut parser = grammar_parser().expect("the public tree uses this grammar");
    let projected_tree =
        parse(&mut parser, &projected).expect("tree-sitter returned a tree before");
    let mut report = inspect_tree(&projected_tree, &projected);
    let position = |row: usize, column: usize| {
        let start: usize = projected
            .split_inclusive(|b| *b == b'\n')
            .take(row)
            .map(<[u8]>::len)
            .sum();
        let at = origins[start + column];
        let point = tree::point(source, at);
        (point.row, point.column)
    };
    if let Some((_, row, column)) = &mut report.first_problem {
        (*row, *column) = position(*row, *column);
    }
    for violation in &mut report.violations {
        (violation.row, violation.column) = position(violation.row, violation.column);
    }
    report.root_end = origins[report.root_end];
    report.len = source.len();
    report
}

fn inspect_tree(tree: &Tree, source: &[u8]) -> Report {
    let len = source.len();
    let root = tree.root_node();
    let mut report = Report {
        root_has_error: root.has_error(),
        root_end: root.end_byte(),
        len,
        violations: constraints::check(tree, source),
        ..Report::default()
    };
    for node in tree::preorder(root) {
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
    }
    report
}

pub fn json_string(s: &str) -> String {
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

pub fn json_line(path: &str, r: &Report) -> String {
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

pub fn grammar_parser() -> Result<Parser, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_mssql::LANGUAGE.into())
        .map_err(|e| format!("cannot load the T-SQL grammar: {e}"))?;
    Ok(parser)
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
    fn quoted_identifier_changes_validation_without_changing_the_public_tree() {
        for sql in [
            "SELECT 1 WHERE 'a' LIKE \"a%\";",
            "SELECT 1 WHERE 'a' IN (\"a\", \"b\");",
            "EXEC p \"a'b\"\"c\";",
            "SELECT \"\";",
        ] {
            let mut parser = grammar_parser().unwrap();
            let tree = parse(&mut parser, sql.as_bytes()).unwrap();
            let before = tree.root_node().to_sexp();
            let off = inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off);
            assert!(off.ok(), "{sql}: {off:?}");
            assert_eq!(tree.root_node().to_sexp(), before);
        }
        for sql in [
            "SELECT * FROM \"t\";",
            "CREATE TABLE \"t\" (id int);",
            "SELECT \"t\".id FROM t;",
            "SELECT \"open",
        ] {
            let mut parser = grammar_parser().unwrap();
            let tree = parse(&mut parser, sql.as_bytes()).unwrap();
            assert!(
                !inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off).ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn inline_setting_overrides_initial_setting_and_maps_diagnostics() {
        let sql = "SET QUOTED_IDENTIFIER OFF; SELECT \"한'글\";\nSELECT * FROM \"bad\";";
        let mut parser = grammar_parser().unwrap();
        let tree = parse(&mut parser, sql.as_bytes()).unwrap();
        let report = inspect(&tree, sql.as_bytes());
        assert!(!report.ok(), "{report:?}");
        assert_eq!(report.len, sql.len());
        assert_eq!(report.root_end, sql.len());
        assert_eq!(report.first_problem.unwrap().1, 1);
        let sql = "SET QUOTED_IDENTIFIER ON; SELECT * FROM \"t\";";
        let tree = parse(&mut parser, sql.as_bytes()).unwrap();
        assert!(inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off).ok());
        let sql = "SELECT \"한'글\"; CREATE FULLTEXT INDEX ON dbo.t(c) KEY INDEX i WITH (CHANGE_TRACKING = AUTO, CHANGE_TRACKING = OFF);";
        let tree = parse(&mut parser, sql.as_bytes()).unwrap();
        let report = inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].row, 0);
        assert_eq!(
            report.violations[0].column,
            sql.rfind("CHANGE_TRACKING").unwrap()
        );
    }

    #[test]
    fn module_setting_is_captured_and_dynamic_sql_does_not_change_the_outer_batch() {
        for sql in [
            "CREATE PROCEDURE dbo.p AS SET QUOTED_IDENTIFIER OFF; SELECT * FROM \"t\";",
            "CREATE PROCEDURE dbo.p AS SET QUOTED_IDENTIFIER ON; SELECT * FROM \"t\";",
            "EXEC ('SET QUOTED_IDENTIFIER OFF;'); SELECT * FROM \"t\";",
            "EXEC ('SET QUOTED_IDENTIFIER ON;'); SELECT * FROM \"t\";",
        ] {
            let mut parser = grammar_parser().unwrap();
            let tree = parse(&mut parser, sql.as_bytes()).unwrap();
            assert!(inspect(&tree, sql.as_bytes()).ok(), "{sql}");
            assert!(
                !inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off).ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn off_strings_cannot_be_convert_types_or_empty_bulk_paths() {
        for (sql, expected) in [
            ("SELECT CONVERT(\"int\", 1);", "gap-call-shape"),
            ("BULK INSERT dbo.t FROM \"\";", "bulk-path-empty"),
        ] {
            let mut parser = grammar_parser().unwrap();
            let tree = parse(&mut parser, sql.as_bytes()).unwrap();
            let report =
                inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off);
            assert_eq!(
                report.violations.iter().map(|v| v.rule).collect::<Vec<_>>(),
                [expected],
                "{sql}: {report:?}"
            );
        }
    }

    #[test]
    fn string_only_statement_arguments_follow_the_session_setting() {
        for sql in [
            "RAISERROR(\"message\",16,1);",
            "WAITFOR DELAY \"00:00:01\";",
            "EXEC (\"SELECT 1\");",
        ] {
            let mut parser = grammar_parser().unwrap();
            let tree = parse(&mut parser, sql.as_bytes()).unwrap();
            assert_eq!(
                inspect(&tree, sql.as_bytes())
                    .violations
                    .iter()
                    .map(|v| v.rule)
                    .collect::<Vec<_>>(),
                ["quoted-identifier-string"],
                "{sql}"
            );
            assert!(
                inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off).ok(),
                "{sql}"
            );
        }
        assert!(check("EXEC p \"value\";").ok());
    }

    #[test]
    fn collation_names_are_unquoted_under_both_settings() {
        for name in [
            "\"Latin1_General_CI_AS\"",
            "[Latin1_General_CI_AS]",
            "'Latin1_General_CI_AS'",
        ] {
            let sql = format!("SELECT 'a' COLLATE {name};");
            let mut parser = grammar_parser().unwrap();
            let tree = parse(&mut parser, sql.as_bytes()).unwrap();
            for setting in [QuotedIdentifier::On, QuotedIdentifier::Off] {
                let report = inspect_with_quoted_identifier(&tree, sql.as_bytes(), setting);
                assert!(!report.ok(), "{sql}: {report:?}");
                assert_eq!(
                    report.violations.iter().map(|v| v.rule).collect::<Vec<_>>(),
                    ["collation-name"]
                );
            }
        }
        assert!(check("SELECT 'a' COLLATE Latin1_General_CI_AS;").ok());
    }

    #[test]
    fn quoted_filegroup_strings_keep_the_engine_accepted_placement_slots() {
        for value in [
            "\"default\"",
            "\"DEFAULT\"",
            "\"PRIMARY\"",
            "\"fg\"",
            "\"NULL\"",
            "\"null\"",
            "'default'",
            "'NULL'",
        ] {
            for sql in [
                format!("CREATE INDEX i ON dbo.t(c) ON {value};"),
                format!("CREATE INDEX i ON dbo.t(c) FILESTREAM_ON {value};"),
                format!("DROP INDEX i ON dbo.t WITH (MOVE TO {value});"),
                format!("DROP INDEX i ON dbo.t WITH (FILESTREAM_ON {value});"),
                format!("ALTER TABLE dbo.t DROP CONSTRAINT c WITH (MOVE TO {value});"),
                format!("CREATE TABLE dbo.t (c int INDEX i NONCLUSTERED FILESTREAM_ON {value});"),
            ] {
                let mut parser = grammar_parser().unwrap();
                let tree = parse(&mut parser, sql.as_bytes()).unwrap();
                for setting in [QuotedIdentifier::On, QuotedIdentifier::Off] {
                    let report = inspect_with_quoted_identifier(&tree, sql.as_bytes(), setting);
                    assert!(report.ok(), "{sql} ({setting:?}): {report:?}");
                }
            }
        }
    }

    #[test]
    fn empty_filegroup_names_follow_the_placement_context() {
        for value in ["''", "\"\""] {
            for sql in [
                format!("CREATE INDEX i ON dbo.t(c) ON {value};"),
                format!("DROP INDEX i ON dbo.t WITH (MOVE TO {value});"),
                format!("DROP INDEX i ON dbo.t WITH (FILESTREAM_ON {value});"),
                format!("ALTER TABLE dbo.t DROP CONSTRAINT c WITH (MOVE TO {value});"),
            ] {
                let mut parser = grammar_parser().unwrap();
                let tree = parse(&mut parser, sql.as_bytes()).unwrap();
                for setting in [QuotedIdentifier::On, QuotedIdentifier::Off] {
                    assert!(
                        !inspect_with_quoted_identifier(&tree, sql.as_bytes(), setting).ok(),
                        "{sql} ({setting:?})"
                    );
                }
            }
            for sql in [
                format!("CREATE INDEX i ON dbo.t(c) FILESTREAM_ON {value};"),
                format!("CREATE TABLE dbo.t (c int INDEX i NONCLUSTERED FILESTREAM_ON {value});"),
            ] {
                let mut parser = grammar_parser().unwrap();
                let tree = parse(&mut parser, sql.as_bytes()).unwrap();
                assert!(
                    inspect_with_quoted_identifier(&tree, sql.as_bytes(), QuotedIdentifier::Off)
                        .ok(),
                    "{sql}"
                );
                assert_eq!(inspect(&tree, sql.as_bytes()).ok(), value == "''", "{sql}");
            }
        }
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
}
