//! Fresh-versus-incremental comparison over edit histories.
//!
//! Input: JSON lines `{"id": ..., "source": ..., "edits": [{"find": ..., "replace": ...}]}`. Each edit
//! replaces the first occurrence of `find` in the current text, cumulatively. After every edit the
//! old tree is edited and reparsed incrementally, and the result must equal a fresh parse of the
//! same text: same nodes, kinds, fields, byte ranges and error/missing/extra flags.

use serde_json::Value;
use tree_sitter::{InputEdit, Parser, Point, Tree};

#[derive(Debug)]
pub struct History {
    pub id: String,
    pub source: String,
    pub edits: Vec<(String, String)>,
}

pub fn read_histories(text: &str) -> Result<Vec<History>, String> {
    let field = |v: &Value, key: &str, line: usize| -> Result<String, String> {
        v[key]
            .as_str()
            .map(String::from)
            .ok_or(format!("line {line}: missing string \"{key}\""))
    };
    let mut histories = Vec::new();
    for (i, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let n = i + 1;
        let v: Value = serde_json::from_str(line).map_err(|e| format!("line {n}: {e}"))?;
        let edits = v["edits"]
            .as_array()
            .ok_or(format!("line {n}: missing array \"edits\""))?
            .iter()
            .map(|e| Ok((field(e, "find", n)?, field(e, "replace", n)?)))
            .collect::<Result<_, String>>()?;
        histories.push(History {
            id: field(&v, "id", n)?,
            source: field(&v, "source", n)?,
            edits,
        });
    }
    Ok(histories)
}

/// Row and byte column of a byte offset, as tree-sitter counts them.
fn point(text: &str, byte: usize) -> Point {
    let before = &text.as_bytes()[..byte];
    let row = before.iter().filter(|&&b| b == b'\n').count();
    let line_start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    Point {
        row,
        column: byte - line_start,
    }
}

fn dump(tree: &Tree) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    let mut depth = 0usize;
    loop {
        let node = cursor.node();
        out.push(format!(
            "{depth} {} {} {}-{} e{} m{} x{}",
            cursor.field_name().unwrap_or("-"),
            node.kind(),
            node.start_byte(),
            node.end_byte(),
            u8::from(node.is_error()),
            u8::from(node.is_missing()),
            u8::from(node.is_extra()),
        ));
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return out;
            }
            depth -= 1;
        }
    }
}

/// The 1-based steps whose incremental tree differs from a fresh parse.
pub fn check_history(parser: &mut Parser, h: &History) -> Result<Vec<usize>, String> {
    let parse = |p: &mut Parser, text: &str, old: Option<&Tree>| {
        p.parse(text, old)
            .ok_or(format!("{}: tree-sitter returned no tree", h.id))
    };
    let mut text = h.source.clone();
    let mut tree = parse(parser, &text, None)?;
    let mut differing = Vec::new();
    for (i, (find, replace)) in h.edits.iter().enumerate() {
        let step = i + 1;
        let start = text
            .find(find.as_str())
            .ok_or(format!("{}: step {step}: text to replace not found", h.id))?;
        let new_text = format!("{}{replace}{}", &text[..start], &text[start + find.len()..]);
        tree.edit(&InputEdit {
            start_byte: start,
            old_end_byte: start + find.len(),
            new_end_byte: start + replace.len(),
            start_position: point(&text, start),
            old_end_position: point(&text, start + find.len()),
            new_end_position: point(&new_text, start + replace.len()),
        });
        let incremental = parse(parser, &new_text, Some(&tree))?;
        let fresh = parse(parser, &new_text, None)?;
        if dump(&incremental) != dump(&fresh) {
            differing.push(step);
        }
        tree = incremental;
        text = new_text;
    }
    Ok(differing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> Parser {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
        p
    }

    #[test]
    fn point_counts_rows_and_byte_columns() {
        let text = "ab\nc한d\n";
        assert_eq!(point(text, 0), Point { row: 0, column: 0 });
        assert_eq!(point(text, 3), Point { row: 1, column: 0 });
        assert_eq!(point(text, 7), Point { row: 1, column: 4 }); // after "c한" (1 + 3 bytes)
    }

    #[test]
    fn a_reparse_without_the_edit_is_detected() {
        // Reusing the old tree without telling tree-sitter about the edit yields stale ranges;
        // the dump comparison must see that, or it could not catch a real incremental bug.
        let mut p = parser();
        let old = p.parse("SELECT a FROM dbo.t;\n", None).unwrap();
        let new_text = "SELECT abc, d FROM dbo.t;\n";
        let stale = p.parse(new_text, Some(&old)).unwrap();
        let fresh = p.parse(new_text, None).unwrap();
        assert_ne!(dump(&stale), dump(&fresh));
    }

    #[test]
    fn simple_edits_match_a_fresh_parse() {
        let h = History {
            id: "t".into(),
            source: "SELECT a FROM dbo.t;\nSELECT b FROM dbo.u;\n".into(),
            edits: vec![
                ("a FROM".into(), "a, c FROM".into()),
                ("dbo.u".into(), "".into()),
            ],
        };
        assert_eq!(
            check_history(&mut parser(), &h).unwrap(),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn hint_number_boundaries_match_fresh_parses() {
        let long_good = format!("SPATIAL_WINDOW_MAX_CELLS={}1", "0".repeat(127));
        let long_bad = format!("SPATIAL_WINDOW_MAX_CELLS={}1", "0".repeat(128));
        for (good, bad) in [
            (long_good.as_str(), long_bad.as_str()),
            (
                "SPATIAL_WINDOW_MAX_CELLS=1ROWLOCK",
                "SPATIAL_WINDOW_MAX_CELLS=1e0ROWLOCK",
            ),
            (
                "SPATIAL_WINDOW_MAX_CELLS=1ROWLOCK",
                "SPATIAL_WINDOW_MAX_CELLS=1.0ROWLOCK",
            ),
            (
                "SPATIAL_WINDOW_MAX_CELLS=00XLOCK",
                "SPATIAL_WINDOW_MAX_CELLS=0XLOCK",
            ),
            ("SPATIAL_WINDOW_MAX_CELLS=1", "SPATIAL_WINDOW_MAX_CELLS=0x1"),
            (
                "SPATIAL_WINDOW_MAX_CELLS=2147483647",
                "SPATIAL_WINDOW_MAX_CELLS=2147483648",
            ),
            (
                "SPATIAL_WINDOW_MAX_CELLS=1/* outer /* inner */ end */ROWLOCK",
                "SPATIAL_WINDOW_MAX_CELLS=1/* unterminated",
            ),
            ("INDEX(2147483647)", "INDEX(2147483648)"),
            ("INDEX(00000000001)", "INDEX(000000000001)"),
            ("INDEX=ix", "INDEX=(ix)"),
            ("FORCESEEK(ix(a))", "FORCESEEK(ix())"),
            ("FORCESEEK(2(a))", "FORCESEEK(-1(a))"),
        ] {
            let source = format!("SELECT N'한글' AS anchor;\nSELECT * FROM dbo.t WITH ({good});\n");
            let mut p = parser();
            assert!(
                !p.parse(&source, None).unwrap().root_node().has_error(),
                "{good}"
            );
            let invalid = source.replace(good, bad);
            assert!(
                p.parse(&invalid, None).unwrap().root_node().has_error(),
                "{bad}"
            );
            let h = History {
                id: good.into(),
                source,
                edits: vec![(good.into(), bad.into()), (bad.into(), good.into())],
            };
            assert!(check_history(&mut p, &h).unwrap().is_empty(), "{good}");
        }
    }

    #[test]
    fn missing_edit_target_stops_without_a_verdict() {
        let h = History {
            id: "t".into(),
            source: "SELECT 1;".into(),
            edits: vec![("x".into(), "y".into())],
        };
        assert!(
            check_history(&mut parser(), &h)
                .unwrap_err()
                .contains("not found")
        );
    }

    #[test]
    fn reads_json_lines() {
        let text = "{\"id\":\"a\",\"source\":\"SELECT 1;\",\"edits\":[{\"find\":\"1\",\"replace\":\"2\"}]}\n\n";
        let h = read_histories(text).unwrap();
        assert_eq!((h.len(), h[0].edits[0].1.as_str()), (1, "2"));
        assert!(read_histories("{\"id\":1}").is_err());
    }
    #[test]
    fn ddl_table_boundaries_recover_incrementally() {
        let cases: Vec<Value> = include_str!("../../../oracle/probes/ddl-table.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let boundaries = [
            ("column-online-off", "ONLINE = OFF", "ONLINE ="),
            ("column-xml-content", "CONTENT dbo.schemas", "CONTENT"),
            ("drop-mixed", "COLUMN IF EXISTS c", "COLUMN IF EXISTS"),
            ("period-add", "vf,vt", "vf"),
            ("period-nested-comment", " c*/", " c"),
            ("switch-wait", "TO dbo.archive", "TO"),
            (
                "system-versioning",
                "HISTORY_TABLE = dbo.hist",
                "HISTORY_TABLE = dbo.",
            ),
            ("index-rebuild-all", "3 TO 5", "3 TO"),
            ("index-rebuild-one", "PARTITION = 2", "PARTITION ="),
            ("column-default-values", "WITH VALUES", "WITH"),
            ("constraint-fillfactor", "FILLFACTOR = 90", "FILLFACTOR ="),
            ("computed-foreign", "REFERENCES dbo.p(id)", "REFERENCES"),
            ("create-temporal", "GENERATED ALWAYS AS", "GENERATED ALWAYS"),
            (
                "create-column-set",
                "COLUMN_SET FOR ALL_SPARSE_COLUMNS",
                "COLUMN_SET FOR",
            ),
            (
                "memory-add-inline-hash",
                "BUCKET_COUNT = 256",
                "BUCKET_COUNT =",
            ),
            ("index-xml-add", "MAXLENGTH (20)", "MAXLENGTH ()"),
            ("create-connection", "dbo.p TO dbo.q", "dbo.p TO"),
            (
                "create-filetable",
                "FILETABLE_DIRECTORY = 'docs'",
                "FILETABLE_DIRECTORY =",
            ),
            (
                "filter-null",
                "FILTER_PREDICATE = NULL",
                "FILTER_PREDICATE =",
            ),
        ];
        let mut p = parser();
        for (id, find, replace) in boundaries {
            let case = cases
                .iter()
                .find(|c| c["id"] == format!("ddl-table/{id}"))
                .unwrap();
            let source = format!("SELECT N'한글';\n{}", case["source"].as_str().unwrap());
            assert!(
                !p.parse(&source, None).unwrap().root_node().has_error(),
                "{id}: initial"
            );
            let broken = source.replacen(find, replace, 1);
            assert!(
                p.parse(&broken, None).unwrap().root_node().has_error(),
                "{id}: broken"
            );
            assert_eq!(broken.replacen(replace, find, 1), source, "{id}: restored");
            let history = History {
                id: id.into(),
                source,
                edits: vec![(find.into(), replace.into()), (replace.into(), find.into())],
            };
            assert!(check_history(&mut p, &history).unwrap().is_empty(), "{id}");
        }
    }

    #[test]
    fn database_ddl_boundaries_recover_incrementally() {
        let cases: Vec<Value> = include_str!("../../../oracle/probes/ddl-database.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let boundaries = [
            ("files/positive-005", "15 %", "15"),
            (
                "database-set/positive-001",
                "STALE_QUERY_THRESHOLD_DAYS = 14",
                "STALE_QUERY_THRESHOLD_DAYS =",
            ),
            ("database-set/positive-019", "SAFETY FULL", "SAFETY"),
            (
                "create/positive-004",
                "FILEGROUP probe_fg DEFAULT",
                "FILEGROUP probe_fg",
            ),
            ("create/positive-009", "FOR ATTACH", "FOR"),
            (
                "create/positive-012",
                "AS SNAPSHOT OF ProbeStore",
                "AS SNAPSHOT OF",
            ),
            (
                "security-ddl/positive-001",
                "SERVER CERTIFICATE probe_cert",
                "SERVER CERTIFICATE",
            ),
            ("security-ddl/positive-012", "STATE = OFF", "STATE ="),
            (
                "ag/positive-018",
                "READ_ONLY_ROUTING_LIST = ('probe-other', 'probe-third')",
                "READ_ONLY_ROUTING_LIST = ()",
            ),
            ("ag/positive-021", "PORT = 1444", "PORT ="),
            ("server/positive-002", "4 TO 6", "4 TO"),
            ("server/positive-011", "SIZE = 12 GB", "SIZE ="),
            ("server/positive-020", "REMOVE WHERE", "REMOVE"),
        ];
        let mut p = parser();
        for (id, find, replace) in boundaries {
            let case = cases
                .iter()
                .find(|c| c["id"] == format!("ddl-database/{id}"))
                .unwrap();
            let source = format!("SELECT N'한글';\n{}", case["source"].as_str().unwrap());
            assert!(source.contains(find), "{id}: edit target");
            let h = History {
                id: id.into(),
                source,
                edits: vec![(find.into(), replace.into()), (replace.into(), find.into())],
            };
            assert!(check_history(&mut p, &h).unwrap().is_empty(), "{id}");
        }
    }
}
