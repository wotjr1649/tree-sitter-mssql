use super::Violation;
use super::util::*;
use crate::tree::{has_child, named_children};
use tree_sitter::Node;

/// Table hint names SQL Server knows (`hint-rules/one-*`; FASTFIRSTROW and other names are Msg 321).
pub(super) const TABLE_HINTS: &[&str] = &[
    "NOLOCK",
    "READUNCOMMITTED",
    "UPDLOCK",
    "REPEATABLEREAD",
    "SERIALIZABLE",
    "HOLDLOCK",
    "READCOMMITTED",
    "READCOMMITTEDLOCK",
    "READPAST",
    "ROWLOCK",
    "PAGLOCK",
    "TABLOCK",
    "TABLOCKX",
    "XLOCK",
    "NOWAIT",
    "SNAPSHOT",
    "NOEXPAND",
    "FORCESCAN",
    "FORCESEEK",
    "INDEX",
    "SPATIAL_WINDOW_MAX_CELLS",
    "KEEPIDENTITY",
    "KEEPDEFAULTS",
    "IGNORE_CONSTRAINTS",
    "IGNORE_TRIGGERS",
];

/// Hints of which at most one may appear in a list (Msg 1047; FORCESEEK with FORCESCAN, Msg 10746),
/// after READUNCOMMITTED -> NOLOCK and SERIALIZABLE -> HOLDLOCK. Every pair of the 28 hints in
/// `hint-rules/pair-*` conflicts exactly when both are in one of these groups.
pub(super) const HINT_GROUPS: &[&[&str]] = &[
    &["PAGLOCK", "NOLOCK", "ROWLOCK", "TABLOCK", "TABLOCKX"],
    &[
        "HOLDLOCK",
        "NOLOCK",
        "READCOMMITTED",
        "READCOMMITTEDLOCK",
        "REPEATABLEREAD",
        "SNAPSHOT",
    ],
    &["NOLOCK", "UPDLOCK", "XLOCK"],
    &["FORCESEEK", "FORCESCAN"],
];

/// A table hint list, `WITH ( ... )` on a table or `OPTION (TABLE HINT (alias, ...))`. Evidence:
/// SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set `hint-rules` (every hint alone,
/// every pair, every hint on each DML target) and the hint-*, pair-*, table-hints/* cases:
/// - an unknown name (Msg 321), and conflicting hints (`HINT_GROUPS`);
/// - FORCESEEK with an index and columns next to INDEX or another FORCESEEK (Msg 10747/10748);
///   FORCESEEK on index 0 (Msg 10749); SPATIAL_WINDOW_MAX_CELLS twice (Msg 1017);
/// - on the target of UPDATE, DELETE, INSERT or MERGE (`WITH` only, not `OPTION`): NOLOCK or
///   READUNCOMMITTED (Msg 1065); INDEX, FORCESEEK, FORCESCAN except on MERGE (Msg 1069, 10724,
///   10745); READPAST and READCOMMITTEDLOCK on INSERT (Msg 4102, 4140).
pub(super) fn table_hints(list: Node, source: &[u8], out: &mut Vec<Violation>) {
    let children = named_children(list);
    if list.kind() == "query_hint" && !has_child(list, "keyword_table") {
        return;
    }
    // After a rowset function (`OPENJSON(@j) WITH (ROWLOCK int)`) the list is a column schema that
    // parses like hints; the engine accepts it (`table-hints/hint-keyword-schema`).
    let after_function = list.syntax_prev_named_sibling().is_some_and(|r| {
        r.kind() == "relation"
            && r.syntax_named_child(0)
                .is_some_and(|n| n.kind() == "invocation")
    });
    if after_function {
        return;
    }
    let target = dml_target(list);
    let mut seen = std::collections::BTreeSet::new();
    let (mut index, mut seek, mut seek_with_columns, mut spatial) = (false, 0, 0, 0);
    for item in children.iter().filter(|n| n.kind() == "table_hint_item") {
        let Some(first) = item.syntax_named_child(0) else {
            continue;
        };
        let name = upper_text(first, source);
        if !TABLE_HINTS.contains(&name.as_str()) {
            out.push(violation(
                "table-hint",
                format!("unknown table hint {name}"),
                *item,
            ));
            continue;
        }
        let name = match name.as_str() {
            "READUNCOMMITTED" => "NOLOCK".to_string(),
            "SERIALIZABLE" => "HOLDLOCK".to_string(),
            _ => name,
        };
        let with_arguments = has_child(*item, "(");
        match name.as_str() {
            "INDEX" => index = true,
            "SPATIAL_WINDOW_MAX_CELLS" => spatial += 1,
            "FORCESEEK" => {
                seek += 1;
                if with_arguments {
                    seek_with_columns += 1;
                    let zero = item.syntax_named_child(1).is_some_and(|n| {
                        n.kind() == "literal" && upper_text(n, source).bytes().all(|b| b == b'0')
                    });
                    if zero {
                        out.push(violation(
                            "table-hint",
                            "FORCESEEK cannot use index 0".into(),
                            *item,
                        ));
                    }
                }
            }
            _ => {}
        }
        if let Some(statement) = target {
            let refused = match name.as_str() {
                "NOLOCK" => true,
                "INDEX" | "FORCESEEK" | "FORCESCAN" => statement != "merge",
                "READPAST" | "READCOMMITTEDLOCK" => statement == "insert",
                _ => false,
            };
            if refused {
                out.push(violation(
                    "table-hint",
                    format!(
                        "{name} cannot be used on the target of {}",
                        statement.to_uppercase()
                    ),
                    *item,
                ));
            }
        }
        seen.insert(name);
    }
    let conflict = HINT_GROUPS
        .iter()
        .any(|group| seen.iter().filter(|n| group.contains(&n.as_str())).count() > 1);
    let detail = if conflict {
        "conflicting table hints"
    } else if seek_with_columns > 0 && (index || seek > 1) {
        "FORCESEEK with an index cannot be combined with INDEX or another FORCESEEK"
    } else if spatial > 1 {
        "SPATIAL_WINDOW_MAX_CELLS appears twice"
    } else {
        return;
    };
    out.push(violation("table-hint", detail.into(), list));
}

/// The statement whose write target carries this `WITH ( ... )` list, if any: the list is a direct
/// child of UPDATE, DELETE or INSERT, or of MERGE before USING (a hint on the USING source is not a
/// target hint).
pub(super) fn dml_target(list: Node) -> Option<&'static str> {
    if list.kind() != "table_hint" {
        return None;
    }
    let statement = match list.parent()?.kind() {
        "update" => "update",
        "delete" => "delete",
        "insert" => "insert",
        "merge" => "merge",
        _ => return None,
    };
    if std::iter::successors(list.syntax_prev_sibling(), SyntaxNode::syntax_prev_sibling)
        .any(|n| n.kind() == "keyword_using")
    {
        return None;
    }
    Some(statement)
}

/// `OPTION (TABLE HINT (alias, ...), TABLE HINT (alias, ...))` names each alias once. Evidence: SQL
/// Server 2025 LocalDB 17.0.1000.7, levels 100-170: Msg 8720 for `hint-rules/option-same-alias-twice`;
/// two different aliases accepted (`hint-rules/option-two-lists`).
pub(super) fn table_hint_aliases(option: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut seen = std::collections::BTreeSet::new();
    for hint in named_children(option)
        .into_iter()
        .filter(|n| n.kind() == "query_hint")
    {
        let kids = named_children(hint);
        // keyword_table, the word HINT (an identifier), then the alias.
        if kids.first().map(|n| n.kind()) != Some("keyword_table") {
            continue;
        }
        let Some(alias) = kids.get(2).map(|n| upper_text(*n, source)) else {
            continue;
        };
        if !seen.insert(alias.clone()) {
            out.push(violation(
                "table-hint",
                format!("TABLE HINT names {alias} twice"),
                hint,
            ));
        }
    }
}
