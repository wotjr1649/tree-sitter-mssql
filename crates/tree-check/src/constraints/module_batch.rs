use super::Violation;
use super::util::*;
use crate::tree::{has_child, named_children};
use tree_sitter::{Node, Tree};

/// A table variable, a table type and a function's RETURNS table cannot have a foreign key, in a
/// column or a table constraint. Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170,
/// oracle set `column-constraints/tablevar-*`, `tabletype-*`, `tvf-*`: Msg 156 at REFERENCES; a
/// CHECK constraint there and a foreign key on a #temp table are accepted.
/// A module must start its batch, and some must end it. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100 and 170, oracle set `batch-rules`: CREATE/ALTER PROCEDURE, VIEW,
/// FUNCTION, TRIGGER and CREATE RULE/DEFAULT after another statement or a label are Msg 111, and
/// inside IF, BEGIN or another module Msg 156; only comments and `;` may come before. A VIEW,
/// FUNCTION, RULE or DEFAULT followed by another statement is Msg 156. A T-SQL procedure or
/// trigger body takes the following statements. The CLR form also rejects them with Msg 156
/// (batch-rules/*clr-{trigger,proc}-then-select, levels 100-170, both sessions).
/// Standalone blocks count as statements: batch-rules/block-then-clr-* (Msg 111)
/// and *clr-*-then-block (Msg 156).
/// CREATE SCHEMA is checked by `surface-shape`; a view
/// inside it is a schema element (accepted, `surface-fix-gaps/schema/view`).
pub(super) fn batch_module(module: Node, out: &mut Vec<Violation>) {
    let statement = module.parent().filter(|p| p.kind() == "statement");
    let batch = statement
        .and_then(|s| s.parent())
        .filter(|b| b.kind() == "batch");
    let (Some(statement), Some(batch)) = (statement, batch) else {
        out.push(violation(
            "batch-module",
            "this statement must start a batch, not sit inside another statement".into(),
            module,
        ));
        return;
    };
    let statements: Vec<Node> = named_children(batch)
        .into_iter()
        .filter(|n| matches!(n.kind(), "statement" | "block"))
        .collect();
    // A view after CREATE SCHEMA in the same batch is one of its schema elements.
    let first_is_schema = statements
        .first()
        .and_then(|s| s.syntax_named_child(0))
        .is_some_and(|n| n.kind() == "create_schema");
    if first_is_schema && module.kind() == "create_view" {
        return;
    }
    if statements.first() != Some(&statement) {
        out.push(violation(
            "batch-module",
            "this statement must be the first in its batch".into(),
            module,
        ));
    }
    let ends_batch = !matches!(
        module.kind(),
        "create_procedure" | "alter_procedure" | "create_trigger" | "alter_trigger"
    ) || module
        .child_by_field_name("body")
        .is_some_and(|body| has_child(body, "external_name"));
    if ends_batch && statements.last() != Some(&statement) {
        out.push(violation(
            "batch-module",
            "this statement must be the last in its batch".into(),
            module,
        ));
    }
}

/// A client splits a script at every GO line, also inside a block comment or a multi-line string
/// or quoted name, so the batch before it ends inside that token. Evidence: SQL Server 2025
/// LocalDB 17.0.1000.7, levels 100 and 170, oracle set `batch-rules/go-in-*`: Msg 113 (comment)
/// and Msg 105 (string); GO in a line comment or as text on a comment line is fine.
pub(super) fn go_line_inside_token(tree: &Tree, source: &[u8], out: &mut Vec<Violation>) {
    let mut line_start = 0;
    for line in source.split_inclusive(|b| *b == b'\n') {
        let text = line.strip_suffix(b"\n").unwrap_or(line);
        let text = text.strip_suffix(b"\r").unwrap_or(text);
        let indent = text
            .iter()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        if is_go_line(&text[indent..]) {
            let go = line_start + indent;
            let node = tree.root_node().descendant_for_byte_range(go, go + 2);
            if let Some(node) = node.filter(|n| n.child_count() == 0 && n.start_byte() < go) {
                out.push(violation(
                    "go-inside-token",
                    "a client splits the batch at this GO line, inside a comment or string".into(),
                    node,
                ));
            }
        }
        line_start += line.len();
    }
}

/// `GO`, an optional count and an optional `--` comment: the line a client treats as a batch separator.
pub(super) fn is_go_line(text: &[u8]) -> bool {
    if text.len() < 2 || !text[..2].eq_ignore_ascii_case(b"go") {
        return false;
    }
    let rest = &text[2..];
    let rest = &rest[rest
        .iter()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count()..];
    let rest = &rest[rest.iter().take_while(|b| b.is_ascii_digit()).count()..];
    let rest = &rest[rest
        .iter()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count()..];
    rest.is_empty() || rest.starts_with(b"--")
}
