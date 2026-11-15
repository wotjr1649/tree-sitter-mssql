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
}
