use super::Violation;
use super::expr::*;
use super::hints::*;
use super::statements::*;
use super::util::*;
use crate::tree::{ancestors, has_child, named_children};
use tree_sitter::Node;

/// R20 gap-forms, LocalDB 17.0.1000.7, levels 100-170: tree-local call boundaries
/// (102/156/174). Qualified user functions are not built-ins; write targets take constants
/// or local variables, not general expressions. Keep the tree for queries and consumers.
pub(super) fn gap_invocation(
    node: Node,
    context: &Context,
    source: &[u8],
    out: &mut Vec<Violation>,
) {
    let mut c = node.walk();
    let args: Vec<_> = node.children_by_field_name("parameter", &mut c).collect();
    let name = invocation_name(node, source).unwrap_or_default();

    let target = context
        .parent()
        .is_some_and(|p| matches!(p.kind(), "delete" | "update"));
    let invalid = match name.as_str() {
        "CONVERT" => args
            .first()
            .is_some_and(|a| string_literal(a.child_by_field_name("value").unwrap_or(*a), source)),
        "[COUNT]" | "\"COUNT\"" | "[COALESCE]" | "\"COALESCE\"" => true,
        "COALESCE" => args.len() < 2,
        "DATENAME" => args.first().is_some_and(|a| {
            let v = a.child_by_field_name("value").unwrap_or(*a);
            let text = upper_text(v, source);
            let text = text
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .or_else(|| text.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                .unwrap_or(&text);
            v.kind() != "field"
                || !matches!(
                    text,
                    "YEAR"
                        | "YY"
                        | "YYYY"
                        | "QUARTER"
                        | "QQ"
                        | "Q"
                        | "MONTH"
                        | "MM"
                        | "M"
                        | "DAYOFYEAR"
                        | "DY"
                        | "Y"
                        | "DAY"
                        | "DD"
                        | "D"
                        | "WEEK"
                        | "WK"
                        | "WW"
                        | "WEEKDAY"
                        | "DW"
                        | "W"
                        | "HOUR"
                        | "HH"
                        | "MINUTE"
                        | "MI"
                        | "N"
                        | "SECOND"
                        | "SS"
                        | "S"
                        | "MILLISECOND"
                        | "MS"
                        | "MICROSECOND"
                        | "MCS"
                        | "NANOSECOND"
                        | "NS"
                        | "TZOFFSET"
                        | "TZ"
                        | "ISO_WEEK"
                        | "ISOWK"
                        | "ISOWW"
                )
        }),
        "OPENQUERY" => {
            args.len() != 2
                || args.first().is_some_and(|a| {
                    let v = a.child_by_field_name("value").unwrap_or(*a);
                    v.kind() != "field"
                        || v.syntax_named_child_count() != 1
                        || v.syntax_named_child(0).is_none_or(|n| {
                            n.kind() != "identifier" || source[n.byte_range()].starts_with(b"@")
                        })
                })
        }
        "OPENROWSET" if target => args
            .first()
            .and_then(|a| a.child_by_field_name("value"))
            .is_some_and(|v| upper_text(v, source) == "BULK"),
        "PREDICT" => {
            context.parent().is_some_and(|p| p.kind() == "relation")
                && context
                    .parent()
                    .and_then(|p| p.syntax_next_named_sibling())
                    .is_none_or(|n| n.kind() != "openjson_schema")
        }
        _ if target => args
            .iter()
            .any(|a| !gap_constant(a.child_by_field_name("value").unwrap_or(*a), source)),
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-call-shape",
            "call arguments or rowset schema do not match this built-in or write target".into(),
            node,
        ));
    }
    if matches!(
        name.as_str(),
        "CONTAINS" | "FREETEXT" | "CONTAINSTABLE" | "FREETEXTTABLE"
    ) || name == "PROPERTY"
        && context.ancestor("invocation").is_some_and(|p| {
            matches!(
                invocation_name(p, source).as_deref(),
                Some("CONTAINS" | "CONTAINSTABLE")
            )
        })
    {
        let pos = if name.ends_with("TABLE") { 2 } else { 1 };
        if let Some(arg) = args.get(pos) {
            let v = arg.child_by_field_name("value").unwrap_or(*arg);
            if v.utf8_text(source).unwrap_or_default().starts_with('"')
                && !quoted_strings(v, source)
            {
                out.push(violation(
                    "gap-fulltext-string",
                    "a double-quoted fulltext string requires QUOTED_IDENTIFIER OFF".into(),
                    v,
                ));
            }
        }
    }
}

pub(super) fn gap_constant(node: Node, source: &[u8]) -> bool {
    node.kind() == "literal"
        || local_variable(node, source)
        || node.kind() == "keyword_default"
        || node.kind() == "unary_expression"
            && node
                .child_by_field_name("operator")
                .is_some_and(|n| n.kind() == "-")
            && node
                .child_by_field_name("operand")
                .is_some_and(|n| n.kind() == "literal")
}

/// R20 comparison/* and static/*: a comparison's operands and a select-list value
/// are scalars, not search conditions (102); EXEC accepts -1 but not +1.
/// A CLR member applies to a scalar, and its name is not a variable or `$` pseudo-column (102).
/// A value list (102, 4145 as an argument) and a rowset function (156/195) are not scalars.
pub(super) fn gap_scalar(node: Node, context: &Context, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "exec_argument" => node
            .child_by_field_name("value")
            .is_some_and(|n| n.kind() == "unary_expression" && has_child(n, "+")),
        "term" => node.child_by_field_name("value").is_some_and(|n| {
            // A subquery is scalar; avoid a root-to-node parent lookup at every nesting level.
            n.kind() != "subquery"
                && context
                    .parent()
                    .is_some_and(|p| p.kind() == "select_expression")
                && is_condition(n, source)
                && !(n.kind() == "binary_expression"
                    && n.child_by_field_name("operator")
                        .is_some_and(|o| o.kind() == "=")
                    && n.child_by_field_name("left").is_some_and(|l| {
                        string_literal(l, source)
                            || l.kind() == "field"
                                && l.syntax_named_child_count() == 1
                                && l.syntax_named_child(0)
                                    .is_some_and(|id| id.kind() == "identifier")
                    }))
        }),
        "binary_expression" => {
            node.child_by_field_name("operator").is_some_and(|n| {
                matches!(
                    n.kind(),
                    "=" | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "<>"
                        | "!="
                        | "!<"
                        | "!>"
                        | "keyword_is"
                        | "is_not"
                        | "keyword_in"
                        | "not_in"
                )
            }) && ["left", "right"].iter().any(|f| {
                node.child_by_field_name(f)
                    .is_some_and(|n| is_condition(n, source))
            })
        }
        // https://learn.microsoft.com/en-us/sql/t-sql/language-elements/expressions-transact-sql
        "list" => context.parent().is_some_and(|p| match p.kind() {
            "parenthesized_expression"
            | "assignment"
            | "cast"
            | "unary_expression"
            | "var_declaration"
            | "return_statement" => true,
            // GROUP BY () is the grand total; GROUP BY (a, b) is not a grouping element.
            "group_by" => node.syntax_named_child_count() > 0,
            "term" => !column_list_argument(p, context.grandparent(), source),
            "binary_expression" => {
                p.child_by_field_name("right") != Some(node)
                    || !p
                        .child_by_field_name("operator")
                        .is_some_and(|o| matches!(o.kind(), "keyword_in" | "not_in"))
            }
            // A VALUES row or IN list item that is itself a list.
            "list" => context.grandparent().is_some_and(|g| {
                // ... or a column inside a full-text or ROLLUP column list.
                matches!(
                    g.kind(),
                    "insert" | "values" | "merge_when_clause" | "binary_expression" | "term"
                )
            }),
            // CHANGETABLE(VERSION t, (columns), (values)); CHANGES takes a version, not a list.
            "changetable" => !has_child(p, "keyword_version"),
            _ => false,
        }),
        // https://learn.microsoft.com/en-us/sql/t-sql/queries/from-transact-sql
        "invocation" => {
            let callee = node
                .syntax_named_child(0)
                .filter(|n| n.kind() == "object_reference");
            let rowset = callee
                .filter(|n| n.syntax_named_child_count() == 1)
                .is_some_and(|n| {
                    matches!(
                        undelimited(&upper_text(n, source)),
                        "OPENJSON" | "OPENROWSET" | "OPENQUERY" | "OPENDATASOURCE" | "OPENXML"
                    )
                });
            // A rowset is a table source or a DML target, before UPDATE's SET.
            rowset
                && !context.parent().is_some_and(|p| match p.kind() {
                    "relation" | "apply_join" | "insert" | "delete" | "merge" => true,
                    "update" => !std::iter::successors(
                        node.syntax_prev_sibling(),
                        SyntaxNode::syntax_prev_sibling,
                    )
                    .any(|n| n.kind() == "keyword_set"),
                    _ => false,
                })
                || callee.is_some_and(|n| member_names(&named_children(n), false, true, source))
        }
        // `@g.@p`; `$` names are graph pseudo-columns of tables, not members of variables.
        "field" => node
            .syntax_named_child(0)
            .filter(|q| q.kind() == "object_reference")
            .zip(node.child_by_field_name("name"))
            .is_some_and(|(q, name)| {
                let mut parts = named_children(q);
                parts.push(name);
                member_names(&parts, has_child(node, "("), false, source)
            }),
        "property_access" | "method_call" => {
            let mut target = node.syntax_named_child(0);
            while let Some(t) = target.filter(|t| t.kind() == "parenthesized_expression") {
                target = t.syntax_named_child(0);
            }
            target.is_some_and(|t| is_condition(t, source))
                || node
                    .child_by_field_name(if node.kind() == "method_call" {
                        "method"
                    } else {
                        "property"
                    })
                    .is_some_and(|n| matches!(source.get(n.start_byte()), Some(b'@' | b'$')))
        }
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-scalar-shape",
            "this position needs a scalar expression or an EXEC constant without unary plus".into(),
            node,
        ));
    }
}

// https://learn.microsoft.com/en-us/sql/t-sql/language-elements/reserved-keywords-transact-sql
// Single words from the SQL Server list. PARSEONLY accepts DISK, DUMP, LOAD, PRECISION and
// SECURITYAUDIT as names, and TRY_CONVERT and the three SEMANTIC* names at level 100.
const RESERVED_NAMES: &[&str] = &[
    "ADD",
    "ALL",
    "ALTER",
    "AND",
    "ANY",
    "AS",
    "ASC",
    "AUTHORIZATION",
    "BACKUP",
    "BEGIN",
    "BETWEEN",
    "BREAK",
    "BROWSE",
    "BULK",
    "BY",
    "CASCADE",
    "CASE",
    "CHECK",
    "CHECKPOINT",
    "CLOSE",
    "CLUSTERED",
    "COALESCE",
    "COLLATE",
    "COLUMN",
    "COMMIT",
    "COMPUTE",
    "CONSTRAINT",
    "CONTAINS",
    "CONTAINSTABLE",
    "CONTINUE",
    "CONVERT",
    "CREATE",
    "CROSS",
    "CURRENT",
    "CURRENT_DATE",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURRENT_USER",
    "CURSOR",
    "DATABASE",
    "DBCC",
    "DEALLOCATE",
    "DECLARE",
    "DEFAULT",
    "DELETE",
    "DENY",
    "DESC",
    "DISTINCT",
    "DISTRIBUTED",
    "DOUBLE",
    "DROP",
    "ELSE",
    "END",
    "ERRLVL",
    "ESCAPE",
    "EXCEPT",
    "EXEC",
    "EXECUTE",
    "EXISTS",
    "EXIT",
    "EXTERNAL",
    "FETCH",
    "FILE",
    "FILLFACTOR",
    "FOR",
    "FOREIGN",
    "FREETEXT",
    "FREETEXTTABLE",
    "FROM",
    "FULL",
    "FUNCTION",
    "GOTO",
    "GRANT",
    "GROUP",
    "HAVING",
    "HOLDLOCK",
    "IDENTITY",
    "IDENTITYCOL",
    "IDENTITY_INSERT",
    "IF",
    "IN",
    "INDEX",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "KILL",
    "LEFT",
    "LIKE",
    "LINENO",
    "MERGE",
    "NATIONAL",
    "NOCHECK",
    "NONCLUSTERED",
    "NOT",
    "NULL",
    "NULLIF",
    "OF",
    "OFF",
    "OFFSETS",
    "ON",
    "OPEN",
    "OPENDATASOURCE",
    "OPENQUERY",
    "OPENROWSET",
    "OPENXML",
    "OPTION",
    "OR",
    "ORDER",
    "OUTER",
    "OVER",
    "PERCENT",
    "PIVOT",
    "PLAN",
    "PRIMARY",
    "PRINT",
    "PROC",
    "PROCEDURE",
    "PUBLIC",
    "RAISERROR",
    "READ",
    "READTEXT",
    "RECONFIGURE",
    "REFERENCES",
    "REPLICATION",
    "RESTORE",
    "RESTRICT",
    "RETURN",
    "REVERT",
    "REVOKE",
    "RIGHT",
    "ROLLBACK",
    "ROWCOUNT",
    "ROWGUIDCOL",
    "RULE",
    "SAVE",
    "SCHEMA",
    "SELECT",
    "SESSION_USER",
    "SET",
    "SETUSER",
    "SHUTDOWN",
    "SOME",
    "STATISTICS",
    "SYSTEM_USER",
    "TABLE",
    "TABLESAMPLE",
    "TEXTSIZE",
    "THEN",
    "TO",
    "TOP",
    "TRAN",
    "TRANSACTION",
    "TRIGGER",
    "TRUNCATE",
    "TSEQUAL",
    "UNION",
    "UNIQUE",
    "UNPIVOT",
    "UPDATE",
    "UPDATETEXT",
    "USE",
    "USER",
    "VALUES",
    "VARYING",
    "VIEW",
    "WAITFOR",
    "WHEN",
    "WHERE",
    "WHILE",
    "WITH",
    "WRITETEXT",
];

/// ROLLUP and CUBE in GROUP BY, and the column-list argument of the full-text and semantic functions.
fn column_list_argument(term: Node, call: Option<Node>, source: &[u8]) -> bool {
    let Some(call) = call.filter(|c| c.kind() == "invocation") else {
        return false;
    };
    let mut c = call.walk();
    let position = call
        .children_by_field_name("parameter", &mut c)
        .position(|p| p == term);
    match invocation_name(call, source).as_deref() {
        // Directly or inside a parenthesized GROUPING SETS item.
        Some("ROLLUP" | "CUBE") => ancestors(call)
            .take_while(|a| !matches!(a.kind(), "subquery" | "query_specification"))
            .any(|a| a.kind() == "group_by"),
        Some("CONTAINS" | "FREETEXT") => position == Some(0),
        Some(
            "CONTAINSTABLE"
            | "FREETEXTTABLE"
            | "SEMANTICKEYPHRASETABLE"
            | "SEMANTICSIMILARITYTABLE",
        ) => position == Some(1),
        _ => false,
    }
}

/// A member after the first part is not a variable; a `$` member is a graph pseudo-column of a
/// table, so it is invalid after a variable or parenthesized receiver, or as a method name.
fn member_names(parts: &[Node], parenthesized: bool, method: bool, source: &[u8]) -> bool {
    let first = |p: &Node| source.get(p.start_byte()).copied();
    let receiver_variable = parts.first().is_some_and(|p| first(p) == Some(b'@'));
    parts.iter().enumerate().skip(1).any(|(i, p)| {
        p.kind() == "identifier"
            && match first(p) {
                Some(b'@') => true,
                Some(b'$') => {
                    receiver_variable
                        || parenthesized
                        || method
                            && (i + 1 == parts.len() || !upper_text(*p, source).eq("$PARTITION"))
                }
                _ => false,
            }
    })
}

/// batch-rules/issue-87-step-5: the first ON is read as the REFERENCES target name and
/// the following action words fall through to _column_constraint's bare identifier arm.
pub(super) fn identifier(node: Node, context: &Context, source: &[u8], out: &mut Vec<Violation>) {
    if node.grammar_name() == "keyword_filestream" {
        let parent = context.parent();
        // ALTER COLUMN ... ADD | DROP takes ROWGUIDCOL, PERSISTED, NOT FOR REPLICATION, SPARSE, MASKED, HIDDEN.
        let altered = parent.is_some_and(|p| p.kind() == "alter_table");
        let filegroup = node
            .syntax_prev_sibling()
            .is_some_and(|n| inline_index_filestream(n, source));
        if altered
            || !filegroup
                && parent
                    .filter(|p| p.kind() == "column_definition")
                    .is_some_and(|column| !filestream_column(node, column, context, source))
        {
            out.push(violation(
                "column-filestream",
                "FILESTREAM needs a varbinary(max) table column, before NULL, ROWGUIDCOL, DEFAULT and constraints".into(),
                node,
            ));
        }
        return;
    }
    // Keyword aliases and SQLCMD paths can share the public identifier kind.
    if !matches!(
        node.grammar_name(),
        "identifier"
            | "_identifier"
            | "_alias_name"
            | "_create_table_identifier"
            | "_graph_identifier"
    ) {
        return;
    }
    let Some(parent) = context.parent() else {
        return;
    };
    let word = node
        .utf8_text(source)
        .unwrap_or_default()
        .to_ascii_uppercase();
    if parent.kind() == "column_definition"
        && parent.child_by_field_name("name") != Some(node)
        // Hidden grammar rules flatten column lists, collation, constraint/index names
        // and FILESTREAM_ON's filegroup into the same column_definition.
        && !node.syntax_prev_sibling().is_some_and(|n| {
            matches!(
                n.grammar_name(),
                "(" | "," | "keyword_collate" | "keyword_constraint" | "keyword_index"
            ) || inline_index_filestream(n, source)
        })
    {
        // https://learn.microsoft.com/en-us/sql/t-sql/statements/create-table-transact-sql
        // HIDDEN is part of generated_always; only these flags can be standalone.
        if !matches!(word.as_str(), "FILESTREAM" | "SPARSE" | "ROWGUIDCOL")
            && !inline_index_filestream(node, source)
        {
            out.push(violation(
                "column-unknown-option",
                "unknown column option".into(),
                node,
            ));
        }
    } else if parent.kind() == "alter_table"
        && word == "FILESTREAM"
        // ALTER COLUMN x ADD | DROP; ALTER TABLE t DROP FILESTREAM drops a constraint of that name.
        && node
            .syntax_prev_sibling()
            .filter(|n| matches!(n.kind(), "keyword_add" | "keyword_drop"))
            .and_then(|n| n.syntax_prev_sibling())
            .and_then(|n| n.syntax_prev_sibling())
            .is_some_and(|n| n.kind() == "keyword_column")
    {
        // ALTER COLUMN ... ADD | DROP has no FILESTREAM property (102).
        out.push(violation(
            "column-filestream",
            "FILESTREAM cannot be added to or dropped from an existing column".into(),
            node,
        ));
    } else if RESERVED_NAMES.binary_search(&word.as_str()).is_ok()
        && !keyword_in_identifier(node, parent, context, &word, source)
    {
        out.push(violation(
            "gap-reserved-name",
            format!("{word} must be delimited when used as a name"),
            node,
        ));
    }
}

// https://learn.microsoft.com/en-us/sql/t-sql/statements/create-table-transact-sql
// batch-rules/column-filestream-*: Msg 102 (5508 for other MAX types); table types and variables reject it.
fn filestream_column(node: Node, column: Node, context: &Context, source: &[u8]) -> bool {
    let table =
        context.ancestor("create_table").is_some() || context.ancestor("alter_table").is_some();
    // SPARSE also lets other MAX types take FILESTREAM (column-filestream-review-*).
    let sparse = named_children(column)
        .into_iter()
        .any(|n| n.kind() == "identifier" && upper_text(n, source) == "SPARSE");
    let max = |ty: Node| {
        ty.child_by_field_name("size")
            .is_some_and(|s| s.kind() == "keyword_max")
    };
    let typed = column
        .child_by_field_name("type")
        .is_some_and(|ty| match ty.kind() {
            "varbinary" => max(ty),
            // A bracketed name: at most schema.name, MAX and nothing after it (102, 117).
            "custom_type" => {
                max(ty)
                    && ty.child_by_field_name("scale").is_none()
                    && ty.child_by_field_name("base_type").is_none()
                    && ty.syntax_named_child(0).is_some_and(|r| {
                        r.syntax_named_child_count() <= 2
                            && r.child_by_field_name("database").is_none()
                            && r.child_by_field_name("name").is_some_and(|n| {
                                sparse || undelimited(&upper_text(n, source)) == "VARBINARY"
                            })
                    })
            }
            _ => sparse && max(ty),
        });
    // Only SPARSE and COLLATE name may come between the type and FILESTREAM.
    let leading =
        std::iter::successors(node.syntax_prev_sibling(), SyntaxNode::syntax_prev_sibling)
            .take_while(|n| column.child_by_field_name("type") != Some(*n))
            .all(|n| {
                n.is_extra()
                    || n.kind() == "keyword_collate"
                    || n.kind() == "identifier"
                        && (upper_text(n, source) == "SPARSE"
                            || n.syntax_prev_sibling()
                                .is_some_and(|p| p.kind() == "keyword_collate"))
            });
    table && typed && leading
}

/// Table types and the return table of a table-valued function (CLR or multi-statement).
fn returned_or_type_table(node: Node, context: &Context) -> bool {
    context.ancestor("create_type").is_some()
        || ancestors(node)
            .find(|a| matches!(a.kind(), "table_var_declaration" | "column_definitions"))
            .and_then(|t| {
                if t.kind() == "column_definitions" {
                    t.parent()
                        .filter(|p| p.kind() == "table_var_declaration")
                        .or(Some(t))
                } else {
                    Some(t)
                }
            })
            .and_then(|t| t.parent())
            .is_some_and(|p| matches!(p.kind(), "create_function" | "alter_function"))
}

fn declared_table(context: &Context) -> bool {
    context
        .ancestor("table_var_declaration")
        .is_some_and(|t| t.parent().is_some_and(|p| p.kind() == "var_declarations"))
}

/// NOT FOR REPLICATION directly after IDENTITY [(seed, increment)], CHECK, or at the end of a
/// REFERENCES clause with its columns and referential actions.
fn replication_placed(column: Node, not: Node, source: &[u8]) -> bool {
    let Some(before) = not.syntax_prev_sibling() else {
        return false;
    };
    let mut c = column.walk();
    // IDENTITY(seed, increment): skip the closing parenthesis.
    let increment = Some(before)
        .filter(|b| b.kind() == ")")
        .and_then(|b| b.syntax_prev_sibling());
    if matches!(before.kind(), "keyword_identity" | "keyword_check")
        || column
            .children_by_field_name("increment", &mut c)
            .any(|n| Some(n) == increment)
    {
        return true;
    }
    // Referenced columns and actions; a standalone NULL or column flag ends the clause. The NULL
    // of SET NULL is inside referential_action, or after a bare SET in error recovery.
    std::iter::successors(Some(before), SyntaxNode::syntax_prev_sibling)
        .find(|n| match n.kind() {
            "object_reference" | "referential_action" | "(" | "," | ")" => false,
            "identifier" => matches!(
                upper_text(*n, source).as_str(),
                "SPARSE" | "ROWGUIDCOL" | "FILESTREAM" | "PERSISTED" | "HIDDEN"
            ),
            "keyword_null" => !n
                .syntax_prev_sibling()
                .is_some_and(|s| upper_text(s, source) == "SET"),
            _ => true,
        })
        .is_some_and(|n| n.kind() == "keyword_references")
}

/// batch-rules/column-sparse-* and column-replication-*: SPARSE occurs once and not in a table
/// type or returned table; NOT FOR REPLICATION follows IDENTITY, CHECK or a REFERENCES clause and
/// is not allowed in table types, table variables or returned tables (102).
pub(super) fn column_options(
    node: Node,
    context: &Context,
    source: &[u8],
    out: &mut Vec<Violation>,
) {
    let restricted = returned_or_type_table(node, context);
    let children = named_children(node);
    // A flag, not the column, index, constraint, collation or referenced-column name.
    let sparse = children
        .iter()
        .filter(|n| {
            n.kind() == "identifier"
                && upper_text(**n, source) == "SPARSE"
                && node.child_by_field_name("name") != Some(**n)
                && !n.syntax_prev_sibling().is_some_and(|p| {
                    matches!(
                        p.grammar_name(),
                        "(" | "," | "keyword_index" | "keyword_constraint" | "keyword_collate"
                    ) || inline_index_filestream(p, source)
                })
        })
        .count();
    let misplaced_replication = children.iter().any(|n| {
        n.kind() == "keyword_replication"
            && (restricted
                || declared_table(context)
                || n.syntax_prev_sibling()
                    .and_then(|f| f.syntax_prev_sibling())
                    .is_none_or(|not| !replication_placed(node, not, source)))
    });
    // In table types and returned tables one SPARSE is measured valid only on a type with a
    // number argument: varchar(40), decimal(10,2), xml(1) (column-option-review-*).
    let numbered = node.child_by_field_name("type").is_some_and(|ty| {
        ty.child_by_field_name("size")
            .or_else(|| ty.child_by_field_name("precision"))
            .is_some_and(|a| a.kind() == "literal")
            || ty.kind() == "keyword_xml"
                && ty.syntax_next_sibling().is_some_and(|n| n.kind() == "(")
    });
    if sparse > 1 || sparse == 1 && restricted && !numbered || misplaced_replication {
        out.push(violation(
            "column-option",
            "SPARSE once outside table types; NOT FOR REPLICATION once after IDENTITY, CHECK or REFERENCES".into(),
            node,
        ));
    }
}

/// A table-level CHECK ... NOT FOR REPLICATION in a table type, table variable or returned table (102).
pub(super) fn constraint_replication(node: Node, context: &Context, out: &mut Vec<Violation>) {
    if has_child(node, "keyword_replication")
        && (returned_or_type_table(node, context) || declared_table(context))
    {
        out.push(violation(
            "column-option",
            "NOT FOR REPLICATION is not allowed in table types, table variables or returned tables"
                .into(),
            node,
        ));
    }
}

fn inline_index_filestream(node: Node, source: &[u8]) -> bool {
    if node.grammar_name() == "keyword_filestream_on" {
        return true;
    }
    // The generic arm can also win lexing for FILESTREAM_ON and its filegroup.
    node.kind() == "identifier"
        && node
            .utf8_text(source)
            .is_ok_and(|text| text.eq_ignore_ascii_case("FILESTREAM_ON"))
        && node
            .syntax_next_sibling()
            .is_some_and(|n| matches!(n.kind(), "identifier" | "literal"))
        && std::iter::successors(node.syntax_prev_sibling(), SyntaxNode::syntax_prev_sibling)
            .find(|n| {
                !matches!(
                    n.kind(),
                    "keyword_clustered"
                        | "keyword_nonclustered"
                        | "keyword_hash"
                        | "ordered_columns"
                        | "with_options"
                        | "keyword_with"
                        | "option"
                        | "on_filegroup"
                )
            })
            .is_some_and(|name| {
                // A column UNIQUE or PRIMARY KEY constraint, or a named inline index.
                matches!(name.kind(), "keyword_unique" | "keyword_key")
                    || name.kind() == "identifier"
                        && name
                            .syntax_prev_sibling()
                            .is_some_and(|n| n.kind() == "keyword_index")
            })
}

fn rowset_modifier(invocation: Node, source: &[u8]) -> bool {
    invocation
        .parent()
        .filter(|n| n.kind() == "term")
        .and_then(|n| n.parent())
        .is_some_and(|n| {
            n.kind() == "invocation" && invocation_name(n, source).as_deref() == Some("OPENROWSET")
        })
}

// The expression grammar can represent a conversion type as a value and alias.
fn conversion_type(argument: Node, source: &[u8]) -> bool {
    let Some(call) = argument.parent().filter(|n| {
        n.kind() == "invocation" && n.child_by_field_name("parameter") == Some(argument)
    }) else {
        return false;
    };
    let mut c = call.walk();
    let parameters: Vec<_> = call.children_by_field_name("parameter", &mut c).collect();
    // CONVERT(type, expression [, style]); DISTINCT and DEFAULT are not arguments here (102/156).
    matches!(
        invocation_name(call, source).as_deref(),
        Some("CONVERT" | "TRY_CONVERT")
    ) && !has_child(argument, "keyword_as")
        && matches!(parameters.len(), 2 | 3)
        && !has_child(call, "keyword_distinct")
        && !parameters.iter().any(|p| p.kind() == "keyword_default")
}

// Some grammar productions use the generic identifier for a fixed syntax word.
fn keyword_in_identifier(
    node: Node,
    parent: Node,
    context: &Context,
    word: &str,
    source: &[u8],
) -> bool {
    let grand = context.grandparent();
    let previous = || node.syntax_prev_sibling().map(|n| n.kind());
    let next = || node.syntax_next_sibling().map(|n| n.kind());
    match (parent.kind(), word) {
        ("object_reference", _) if parent.syntax_child(1).is_none() => {
            match grand.map(|n| n.kind()) {
                Some("invocation") => match word {
                    "COALESCE" | "CONTAINS" | "CONTAINSTABLE" | "CONVERT" | "FREETEXT"
                    | "FREETEXTTABLE" | "IDENTITY" | "LEFT" | "NULLIF" | "OPENDATASOURCE"
                    | "OPENQUERY" | "OPENROWSET" | "OPENXML" | "RIGHT" | "TSEQUAL" | "UPDATE" => {
                        true
                    }
                    "CURRENT_DATE" | "CURRENT_TIME" | "CURRENT_TIMESTAMP" | "DATABASE"
                    | "INSERT" | "TRUNCATE" => context
                        .ancestor_parent("invocation")
                        .is_some_and(|n| n.kind() == "odbc_function_escape"),
                    "BULK" | "ORDER" => grand.is_some_and(|n| rowset_modifier(n, source)),
                    _ => false,
                },
                Some("set_statement") => {
                    parent
                        .syntax_prev_sibling()
                        .is_some_and(|n| n.kind() == "keyword_set")
                        && matches!(
                            word,
                            "IDENTITY_INSERT" | "OFFSETS" | "ROWCOUNT" | "STATISTICS" | "TEXTSIZE"
                        )
                }
                // A nameless TIMESTAMP column can parse its nullability as a type.
                Some("custom_type") if matches!(word, "NULL" | "NOT") => grand.is_some_and(|ty| {
                    ty.syntax_child(1).is_none()
                        && (word == "NULL"
                            || ty
                                .syntax_next_sibling()
                                .is_some_and(|n| n.kind() == "keyword_null"))
                        && ty.parent().is_some_and(|column| {
                            column.kind() == "column_definition"
                                && column.child_by_field_name("type") == Some(ty)
                                && column.child_by_field_name("name").is_some_and(|name| {
                                    matches!(
                                        upper_text(name, source).as_str(),
                                        "TIMESTAMP" | "[TIMESTAMP]" | "\"TIMESTAMP\""
                                    )
                                })
                        })
                }),
                Some("field") => matches!(
                    word,
                    "CURRENT_DATE"
                        | "CURRENT_TIMESTAMP"
                        | "CURRENT_USER"
                        | "SESSION_USER"
                        | "SYSTEM_USER"
                        | "USER"
                ),
                // The static-reference check already disallows NULL in either part.
                Some("object_reference") => {
                    word == "NULL" && grand.is_some_and(|n| has_child(n, "::"))
                }
                _ => false,
            }
        }
        (
            "field",
            "CURRENT_DATE" | "CURRENT_TIMESTAMP" | "CURRENT_USER" | "SESSION_USER" | "SYSTEM_USER"
            | "USER",
        ) => parent.syntax_child(0) == Some(node) && parent.syntax_child(1).is_none(),
        ("field", "DOUBLE" | "NATIONAL") => {
            parent.syntax_child(0) == Some(node)
                && parent.syntax_child(1).is_none()
                && grand.is_some_and(|argument| {
                    argument.kind() == "term"
                        && argument.child_by_field_name("value") == Some(parent)
                        && argument.child_by_field_name("alias").is_some_and(|alias| {
                            matches!(
                                (word, upper_text(alias, source).as_str()),
                                ("DOUBLE", "PRECISION") | ("NATIONAL", "CHAR" | "CHARACTER")
                            )
                        })
                        && conversion_type(argument, source)
                })
        }
        ("field", "IDENTITYCOL" | "ROWGUIDCOL") => parent.child_by_field_name("name") == Some(node),
        ("field", "DEFAULT") => grand.is_some_and(|n| n.kind() == "list"),
        ("field", "DISTINCT") => grand.is_some_and(|n| {
            n.kind() == "unary_expression"
                && n.child_by_field_name("operator")
                    .is_some_and(|op| op.kind() == "keyword_all")
        }),
        ("field", "COMPUTE" | "EXECUTE" | "PROCEDURE" | "TABLE" | "COLUMN") => {
            grand.is_some_and(|n| match n.kind() {
                "dbcc_statement" => word == "COMPUTE",
                "set_statement" => n
                    .syntax_named_child(1)
                    .is_some_and(|option| upper_text(option, source) == "OFFSETS"),
                "binary_expression" => {
                    matches!(word, "TABLE" | "COLUMN")
                        && n.child_by_field_name("left") == Some(parent)
                        && context.ancestor("invocation").is_some_and(|a| {
                            invocation_name(a, source).as_deref() == Some("VECTOR_SEARCH")
                        })
                }
                _ => false,
            })
        }
        ("field", "BULK") | ("term", "UNIQUE") => context
            .ancestor("invocation")
            .is_some_and(|n| invocation_name(n, source).as_deref() == Some("OPENROWSET")),
        ("term", "VARYING") => {
            parent.child_by_field_name("alias") == Some(node)
                && parent
                    .child_by_field_name("value")
                    .filter(|value| {
                        value.kind() == "field"
                            && value
                                .syntax_named_child(0)
                                .filter(|n| n.kind() == "object_reference")
                                .is_none_or(|n| n.syntax_named_child_count() == 1)
                    })
                    .and_then(|value| value.child_by_field_name("name"))
                    .is_some_and(|name| {
                        matches!(
                            upper_text(name, source).as_str(),
                            "CHAR"
                                | "CHARACTER"
                                | "BINARY"
                                | "NCHAR"
                                | "[CHAR]"
                                | "[CHARACTER]"
                                | "[BINARY]"
                                | "[NCHAR]"
                                | "\"CHAR\""
                                | "\"CHARACTER\""
                                | "\"BINARY\""
                                | "\"NCHAR\""
                        )
                    })
                && conversion_type(parent, source)
        }
        ("term", "ASC" | "DESC") => grand.is_some_and(|n| {
            n.kind() == "invocation"
                && invocation_name(n, source).as_deref() == Some("ORDER")
                && rowset_modifier(n, source)
        }),
        (
            "option",
            "ALL" | "CLUSTERED" | "DISTRIBUTED" | "FILE" | "FILLFACTOR" | "FUNCTION" | "GROUP"
            | "IDENTITY" | "ORDER" | "ROWCOUNT",
        ) => parent.child_by_field_name("name") == Some(node),
        ("option", "PERCENT") => {
            previous() == Some("literal") && option_name(parent, source) == "SAMPLE"
        }
        ("option", "BY" | "INDEX") => {
            grand.is_some_and(|n| n.kind() == "with_options")
                && parent.child_by_field_name("value") == Some(node)
                && matches!(
                    (word, option_name(parent, source).as_str()),
                    ("BY", "CLUSTER") | ("INDEX", "CLUSTERED")
                )
        }
        ("option", "ANY" | "ASC" | "CONTINUE" | "DESC" | "PRIMARY" | "SHUTDOWN" | "TABLE") => {
            parent.child_by_field_name("value") == Some(node)
                && grand.is_some_and(|n| {
                    matches!(
                        (word, n.kind()),
                        ("ANY", "alter_broker_priority" | "create_broker_priority")
                            | ("ASC" | "DESC", "option")
                            | ("CONTINUE" | "SHUTDOWN", "with_options")
                            | ("PRIMARY", "alter_database")
                            | ("TABLE", "alter_table")
                    )
                })
        }
        ("permission", _) => true,
        ("query_hint", "DISTRIBUTED" | "GROUP" | "JOIN" | "MERGE" | "ORDER" | "PLAN" | "UNION") => {
            true
        }
        ("alter_database", "CURRENT") => previous() == Some("keyword_database"),
        ("alter_fulltext_index", "FULL" | "UPDATE") => previous() == Some("keyword_start"),
        ("alter_fulltext_index", "OFF") => next().is_none() || next() == Some("with_clause"),
        ("alter_table", "ROWGUIDCOL") => matches!(previous(), Some("keyword_add" | "keyword_drop")),
        ("fulltext_index_column", "COLUMN") => {
            node.syntax_prev_sibling()
                .is_some_and(|n| upper_text(n, source) == "TYPE")
                && parent.child_by_field_name("name") != Some(node)
        }
        ("grant_statement", "PUBLIC") | ("audit_action", "PUBLIC") => {
            matches!(
                previous(),
                Some("keyword_to" | "keyword_from" | "keyword_by" | ",")
            )
        }
        ("grant_statement" | "alter_authorization" | "audit_action", "SCHEMA" | "USER") => {
            previous() == Some("keyword_on") && next() == Some("::")
        }
        _ => false,
    }
}

/// R20 name/qualification, engine 117/156: XML collections have at most schema.name,
/// HISTORY_TABLE has exactly schema.name; XE qualified names have no empty components.
pub(super) fn gap_name(node: Node, context: &Context, source: &[u8], out: &mut Vec<Violation>) {
    let Some(parent) = context.parent() else {
        return;
    };
    let parts = named_children(node).len();
    let mut c = node.walk();
    let dots = node
        .syntax_children(&mut c)
        .filter(|n| n.kind() == ".")
        .count();
    let collection = matches!(
        parent.kind(),
        "create_xml_schema_collection"
            | "alter_xml_schema_collection"
            | "drop_xml_schema_collection"
    );
    let history = parent.kind() == "option" && option_name(parent, source) == "HISTORY_TABLE";
    let xe = matches!(
        parent.kind(),
        "event_session_event" | "event_session_target"
    );
    let xml_null = parent.kind() == "alter_table"
        && node
            .syntax_prev_named_sibling()
            .is_some_and(|n| n.kind() == "keyword_xml")
        && upper_text(node, source) == "NULL";
    if (collection && (dots > 1 || parts != dots + 1))
        || (history && (dots != 1 || parts != 2))
        || (xe && parts != dots + 1)
        || xml_null
    {
        out.push(violation(
            "gap-name-parts",
            "name qualification or XML schema name is invalid in this position".into(),
            node,
        ));
    }
    // XE ACTION names are direct object_reference children too, unlike nested type names.
    if xe && dots > 2 {
        out.push(violation(
            "gap-name-parts",
            "XE event/action names have at most three parts".into(),
            node,
        ));
    }
    if parent.kind() == "drop_xml_schema_collection" && has_child(parent, "keyword_if") {
        out.push(violation(
            "gap-xml-drop",
            "DROP XML SCHEMA COLLECTION has no IF EXISTS clause".into(),
            parent,
        ));
    }
}

/// R20 gap-forms/hints, maxdop, table and index: Msg 1018/304/1080/10737/13539/
/// 35310/10798/122/153/155. These limits depend only on the option's owner and value.
pub(super) fn gap_options(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "table_hint" => {
            let target = dml_target(node);
            let merge_source =
                node.parent().is_some_and(|n| n.kind() == "merge") && target.is_none();
            merge_source
                || !has_child(node, "keyword_with")
                    && (target.is_some() || {
                        named_children(node)
                            .into_iter()
                            .any(|n| n.kind() == "table_hint_item" && has_child(n, "keyword_index"))
                    })
        }
        "query_hint" => {
            let parts: Vec<_> = named_children(node);
            parts
                .first()
                .is_some_and(|n| upper_text(*n, source) == "DISABLE_OPTIMIZED_PLAN_FORCING")
                || parts
                    .first()
                    .is_some_and(|n| upper_text(*n, source) == "MAXDOP")
                    && (parts.len() != 2
                        || !upper_text(parts[1], source)
                            .bytes()
                            .all(|b| b.is_ascii_digit())
                        || !upper_text(parts[1], source)
                            .parse::<u32>()
                            .is_ok_and(|n| n <= 32767))
        }
        "constraint" => {
            has_child(node, "keyword_columnstore")
                && ancestor(node, "table_var_declaration").is_some()
        }
        "with_options" => {
            let mut seen = false;
            node.parent().is_some_and(|p| p.kind() == "create_table")
                && direct_options(node).iter().any(|o| {
                    option_name(*o, source) == "SYSTEM_VERSIONING"
                        && std::mem::replace(&mut seen, true)
                })
        }
        "option" => {
            let name = option_name(node, source);
            let value = node.child_by_field_name("value");
            let table = ancestor(node, "alter_table");
            let index = ancestor(node, "alter_index");
            let column_key = node
                .parent()
                .and_then(|p| p.parent())
                .is_some_and(|p| matches!(p.kind(), "column_definition" | "constraint"));
            let partitions = has_child(node, "keyword_partitions");
            // The engine checks ON PARTITIONS only on the first WITH option (gap-forms/partition).
            let compression_partition = partitions
                && node
                    .syntax_prev_named_sibling()
                    .is_some_and(|n| n.kind() == "keyword_with")
                && index.is_some()
                && (!matches!(name.as_str(), "DATA_COMPRESSION" | "XML_COMPRESSION")
                    || index.is_some_and(|i| {
                        let parts: Vec<_> = named_children(i);
                        !parts.windows(2).any(|w| {
                            w[0].kind() == "keyword_partition" && upper_text(w[1], source) == "ALL"
                        })
                    }));
            let row_key = node.parent().and_then(|p| p.parent()).is_some_and(|p| {
                !has_child(p, "keyword_columnstore")
                    && (has_child(p, "keyword_primary") || has_child(p, "keyword_unique"))
            });
            let bad_key_option = column_key
                && row_key
                && (name == "COMPRESSION_DELAY"
                    || name == "DATA_COMPRESSION"
                        && value.is_some_and(|v| upper_text(v, source).starts_with("COLUMNSTORE")));
            let table_rebuild = table.is_some_and(|t| has_child(t, "keyword_rebuild"))
                && matches!(name.as_str(), "RESUMABLE" | "OPTIMIZE_FOR_SEQUENTIAL_KEY");
            let lock_escalation = table.is_some()
                && value.is_some_and(|n| upper_text(n, source) == "TABLE")
                && name != "LOCK_ESCALATION";
            let policy =
                ancestor(node, "alter_security_policy").is_some() && name == "SCHEMABINDING";
            let external = ancestor(node, "create_external_file_format").is_some()
                && (name == "FORMAT_TYPE"
                    && value.is_some_and(|v| {
                        matches!(upper_text(v, source).as_str(), "JSON" | "RCFILE")
                    })
                    || name == "PARSER_VERSION");
            let source_option = ancestor(node, "alter_external_data_source").is_some()
                && name == "RESOURCE_MANAGER_LOCATION";
            let history = name == "HISTORY_TABLE"
                && value.is_some_and(|v| v.kind() != "object_reference" || !has_child(v, "."));
            compression_partition
                || history
                || (ancestor(node, "create_table")
                    .is_some_and(|t| has_child(t, "keyword_external"))
                    && matches!(name.as_str(), "DATA_SOURCE" | "FILE_FORMAT")
                    && value.is_none_or(|v| {
                        v.kind() != "identifier" || upper_text(v, source).starts_with('@')
                    }))
                || bad_key_option
                || table_rebuild
                || lock_escalation
                || policy
                || external
                || source_option
        }
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-option-shape",
            "option value, placement, range or repetition is not accepted here".into(),
            node,
        ));
    }
    if node.kind() == "with_options"
        && direct_options(node)
            .iter()
            .any(|n| option_name(*n, source) == "ONLINE")
        && ancestor(node, "create_index").is_some_and(|i| {
            i.child_by_field_name("storage")
                .is_some_and(|s| upper_text(s, source) == "JSON")
        })
    {
        out.push(violation(
            "gap-json-index",
            "JSON indexes do not support ONLINE".into(),
            node,
        ));
    }
}

/// R20 gap-forms/blocks, xe, route, cache, window, permission and trigger: LocalDB
/// 17.0.1000.7, levels 100-170. Empty CATCH and final unterminated ENABLE are accepted.
pub(super) fn gap_statement(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let invalid = match node.kind() {
        "block" | "try_block" => !named_children(node)
            .into_iter()
            .any(|n| matches!(n.kind(), "statement" | "block" | "label_statement")),
        "enable_trigger_statement" => node.parent().is_some_and(|s| {
            let next = s.syntax_next_sibling();
            next.is_some_and(|n| n.kind() != ";" && n.kind() != "go_statement")
        }),
        "window_frame" => {
            !has_child(node, "keyword_between")
                && node.syntax_named_child(1).is_some_and(|f| {
                    has_child(f, "keyword_unbounded") && has_child(f, "keyword_following")
                })
        }
        "create_event_session" => !has_child(node, "event_session_event"),
        "alter_event_session" => named_children(node).len() == 6,
        "create_route" | "alter_route" => {
            let options = named_children(node)
                .into_iter()
                .find(|n| n.kind() == "with_clause")
                .map(direct_options)
                .unwrap_or_default();
            let mut seen = std::collections::BTreeSet::new();
            let mut duplicate = false;
            for opt in &options {
                let name = option_name(*opt, source);
                duplicate |= !seen.insert(name);
            }
            duplicate || node.kind() == "create_route" && !seen.iter().any(|s| s == "ADDRESS")
        }
        "alter_database" => {
            let handle = named_children(node)
                .into_iter()
                .find(|n| n.kind() == "keyword_procedure_cache")
                .and_then(|value| value.syntax_next_named_sibling());
            handle
                .is_some_and(|n| n.kind() != "literal" || !upper_text(n, source).starts_with("0X"))
                || has_child(node, "keyword_scoped")
                    && direct_options(node).iter().any(|n| {
                        matches!(
                            option_name(*n, source).as_str(),
                            "ALLOW_BUILTIN_TVF_IN_ALL_COMPAT_LEVELS" | "ALLOW_STALE_VECTOR_INDEX"
                        )
                    })
        }
        "create_index" => {
            node.child_by_field_name("storage")
                .is_some_and(|s| upper_text(s, source) == "JSON")
                && has_child(node, "on_filegroup")
        }
        "create_server_audit" | "alter_server_audit" => {
            let parts: Vec<_> = named_children(node);
            parts.windows(2).any(|w| {
                w[0].kind() == "keyword_to"
                    && matches!(
                        upper_text(w[1], source).as_str(),
                        "URL" | "EXTERNAL_MONITOR"
                    )
            })
        }
        "permission" => {
            if node.parent().is_none_or(|p| p.kind() != "grant_statement") {
                return;
            }
            let words: Vec<_> = named_children(node)
                .into_iter()
                .filter(|n| n.kind() != "list")
                .map(|n| upper_text(n, source))
                .collect();
            let head = words.first().map(String::as_str).unwrap_or_default();
            !matches!(
                head,
                "ALL"
                    | "ALTER"
                    | "AUTHENTICATE"
                    | "BACKUP"
                    | "CHECKPOINT"
                    | "CONNECT"
                    | "CONTROL"
                    | "CREATE"
                    | "DELETE"
                    | "EXECUTE"
                    | "EXEC"
                    | "IMPERSONATE"
                    | "INSERT"
                    | "RECEIVE"
                    | "REFERENCES"
                    | "SELECT"
                    | "SEND"
                    | "SHOWPLAN"
                    | "SHUTDOWN"
                    | "SUBSCRIBE"
                    | "TAKE"
                    | "UNMASK"
                    | "UPDATE"
                    | "VIEW"
                    | "ADMINISTER"
                    | "EXTERNAL"
                    | "ENABLE"
                    | "KILL"
            ) || matches!(
                words.join(" ").as_str(),
                "CREATE APPLICATION ROLE" | "ALTER ANY DATABASE SCOPED CREDENTIAL"
            )
        }
        "trigger_event" => {
            let event = upper_text(node, source);
            !matches!(
                event.as_str(),
                "INSERT" | "UPDATE" | "DELETE" | "ALTER_SERVER_CONFIGURATION"
            ) && !SERVER_ONLY_EVENTS.contains(&event.as_str())
                && !DATABASE_TRIGGER_EVENTS.contains(&event.as_str())
        }
        _ => false,
    };
    if invalid {
        out.push(violation(
            "gap-statement-shape",
            "statement body, event, option or permission shape is rejected by the engine".into(),
            node,
        ));
    }
}

// Fixed event/group names measured by the context/trig-* oracle enumeration.
pub(super) const DATABASE_TRIGGER_EVENTS: &[&str] = &[
    "ADD_INFORMATION_PROTECTION",
    "ADD_ROLE_MEMBER",
    "ADD_SENSITIVITY_CLASSIFICATION",
    "ADD_SIGNATURE",
    "ADD_SIGNATURE_SCHEMA_OBJECT",
    "ALTER_APPLICATION_ROLE",
    "ALTER_ASSEMBLY",
    "ALTER_ASYMMETRIC_KEY",
    "ALTER_AUDIT",
    "ALTER_AUTHORIZATION_DATABASE",
    "ALTER_BROKER_PRIORITY",
    "ALTER_CERTIFICATE",
    "ALTER_COLUMN_ENCRYPTION_KEY",
    "ALTER_DATABASE_AUDIT_SPECIFICATION",
    "ALTER_DATABASE_ENCRYPTION_KEY",
    "ALTER_DATABASE_SCOPED_CONFIGURATION",
    "ALTER_EXTENDED_PROPERTY",
    "ALTER_EXTERNAL_LANGUAGE",
    "ALTER_EXTERNAL_LIBRARY",
    "ALTER_FULLTEXT_CATALOG",
    "ALTER_FULLTEXT_INDEX",
    "ALTER_FULLTEXT_STOPLIST",
    "ALTER_FUNCTION",
    "ALTER_INDEX",
    "ALTER_MASTER_KEY",
    "ALTER_MESSAGE_TYPE",
    "ALTER_PARTITION_FUNCTION",
    "ALTER_PARTITION_SCHEME",
    "ALTER_PLAN_GUIDE",
    "ALTER_PROCEDURE",
    "ALTER_QUEUE",
    "ALTER_REMOTE_SERVICE_BINDING",
    "ALTER_ROLE",
    "ALTER_ROUTE",
    "ALTER_SCHEMA",
    "ALTER_SEARCH_PROPERTY_LIST",
    "ALTER_SECURITY_POLICY",
    "ALTER_SEQUENCE",
    "ALTER_SERVICE",
    "ALTER_SYMMETRIC_KEY",
    "ALTER_TABLE",
    "ALTER_TRIGGER",
    "ALTER_USER",
    "ALTER_VIEW",
    "ALTER_XML_SCHEMA_COLLECTION",
    "BIND_DEFAULT",
    "BIND_RULE",
    "CREATE_APPLICATION_ROLE",
    "CREATE_ASSEMBLY",
    "CREATE_ASYMMETRIC_KEY",
    "CREATE_AUDIT",
    "CREATE_BROKER_PRIORITY",
    "CREATE_CERTIFICATE",
    "CREATE_COLUMN_ENCRYPTION_KEY",
    "CREATE_COLUMN_MASTER_KEY",
    "CREATE_CONTRACT",
    "CREATE_DATABASE_AUDIT_SPECIFICATION",
    "CREATE_DATABASE_ENCRYPTION_KEY",
    "CREATE_DEFAULT",
    "CREATE_EVENT_NOTIFICATION",
    "CREATE_EXTENDED_PROPERTY",
    "CREATE_EXTERNAL_LANGUAGE",
    "CREATE_EXTERNAL_LIBRARY",
    "CREATE_EXTERNAL_STREAM",
    "CREATE_FULLTEXT_CATALOG",
    "CREATE_FULLTEXT_INDEX",
    "CREATE_FULLTEXT_STOPLIST",
    "CREATE_FUNCTION",
    "CREATE_INDEX",
    "CREATE_JSON_INDEX",
    "CREATE_MASTER_KEY",
    "CREATE_MESSAGE_TYPE",
    "CREATE_PARTITION_FUNCTION",
    "CREATE_PARTITION_SCHEME",
    "CREATE_PLAN_GUIDE",
    "CREATE_PROCEDURE",
    "CREATE_QUEUE",
    "CREATE_REMOTE_SERVICE_BINDING",
    "CREATE_ROLE",
    "CREATE_ROUTE",
    "CREATE_RULE",
    "CREATE_SCHEMA",
    "CREATE_SEARCH_PROPERTY_LIST",
    "CREATE_SECURITY_POLICY",
    "CREATE_SEQUENCE",
    "CREATE_SERVICE",
    "CREATE_SPATIAL_INDEX",
    "CREATE_STATISTICS",
    "CREATE_SYMMETRIC_KEY",
    "CREATE_SYNONYM",
    "CREATE_TABLE",
    "CREATE_TRIGGER",
    "CREATE_TYPE",
    "CREATE_USER",
    "CREATE_VECTOR_INDEX",
    "CREATE_VIEW",
    "CREATE_XML_INDEX",
    "CREATE_XML_SCHEMA_COLLECTION",
    "DDL_APPLICATION_ROLE_EVENTS",
    "DDL_ASSEMBLY_EVENTS",
    "DDL_ASYMMETRIC_KEY_EVENTS",
    "DDL_AUTHORIZATION_DATABASE_EVENTS",
    "DDL_BROKER_PRIORITY_EVENTS",
    "DDL_CERTIFICATE_EVENTS",
    "DDL_COLUMN_ENCRYPTION_KEY_EVENTS",
    "DDL_COLUMN_MASTER_KEY_EVENTS",
    "DDL_CONTRACT_EVENTS",
    "DDL_CRYPTO_SIGNATURE_EVENTS",
    "DDL_DATABASE_AUDIT_EVENTS",
    "DDL_DATABASE_AUDIT_SPECIFICATION_EVENTS",
    "DDL_DATABASE_ENCRYPTION_KEY_EVENTS",
    "DDL_DATABASE_LEVEL_EVENTS",
    "DDL_DATABASE_SECURITY_EVENTS",
    "DDL_DEFAULT_EVENTS",
    "DDL_EVENT_NOTIFICATION_EVENTS",
    "DDL_EXTENDED_PROPERTY_EVENTS",
    "DDL_EXTERNAL_LANGUAGE_EVENTS",
    "DDL_EXTERNAL_STREAM_EVENTS",
    "DDL_FIDO_EVENTS",
    "DDL_FULLTEXT_CATALOG_EVENTS",
    "DDL_FULLTEXT_STOPLIST_EVENTS",
    "DDL_FUNCTION_EVENTS",
    "DDL_GDR_DATABASE_EVENTS",
    "DDL_INDEX_EVENTS",
    "DDL_LIBRARY_EVENTS",
    "DDL_MASTER_KEY_EVENTS",
    "DDL_MESSAGE_TYPE_EVENTS",
    "DDL_PARTITION_EVENTS",
    "DDL_PARTITION_FUNCTION_EVENTS",
    "DDL_PARTITION_SCHEME_EVENTS",
    "DDL_PLAN_GUIDE_EVENTS",
    "DDL_PROCEDURE_EVENTS",
    "DDL_QUEUE_EVENTS",
    "DDL_REMOTE_SERVICE_BINDING_EVENTS",
    "DDL_ROLE_EVENTS",
    "DDL_ROUTE_EVENTS",
    "DDL_RULE_EVENTS",
    "DDL_SCHEMA_EVENTS",
    "DDL_SEARCH_PROPERTY_LIST_EVENTS",
    "DDL_SECURITY_POLICY_EVENTS",
    "DDL_SENSITIVITY_EVENTS",
    "DDL_SEQUENCE_EVENTS",
    "DDL_SERVICE_EVENTS",
    "DDL_SSB_EVENTS",
    "DDL_STATISTICS_EVENTS",
    "DDL_SYMMETRIC_KEY_EVENTS",
    "DDL_SYNONYM_EVENTS",
    "DDL_TABLE_EVENTS",
    "DDL_TABLE_VIEW_EVENTS",
    "DDL_TRIGGER_EVENTS",
    "DDL_TYPE_EVENTS",
    "DDL_USER_EVENTS",
    "DDL_VIEW_EVENTS",
    "DDL_XML_SCHEMA_COLLECTION_EVENTS",
    "DENY_DATABASE",
    "DROP_APPLICATION_ROLE",
    "DROP_ASSEMBLY",
    "DROP_ASYMMETRIC_KEY",
    "DROP_AUDIT",
    "DROP_BROKER_PRIORITY",
    "DROP_CERTIFICATE",
    "DROP_COLUMN_ENCRYPTION_KEY",
    "DROP_COLUMN_MASTER_KEY",
    "DROP_CONTRACT",
    "DROP_DATABASE_AUDIT_SPECIFICATION",
    "DROP_DATABASE_ENCRYPTION_KEY",
    "DROP_DEFAULT",
    "DROP_EVENT_NOTIFICATION",
    "DROP_EXTENDED_PROPERTY",
    "DROP_EXTERNAL_LANGUAGE",
    "DROP_EXTERNAL_LIBRARY",
    "DROP_EXTERNAL_STREAM",
    "DROP_FULLTEXT_CATALOG",
    "DROP_FULLTEXT_INDEX",
    "DROP_FULLTEXT_STOPLIST",
    "DROP_FUNCTION",
    "DROP_INDEX",
    "DROP_INFORMATION_PROTECTION",
    "DROP_MASTER_KEY",
    "DROP_MESSAGE_TYPE",
    "DROP_PARTITION_FUNCTION",
    "DROP_PARTITION_SCHEME",
    "DROP_PLAN_GUIDE",
    "DROP_PROCEDURE",
    "DROP_QUEUE",
    "DROP_REMOTE_SERVICE_BINDING",
    "DROP_ROLE",
    "DROP_ROLE_MEMBER",
    "DROP_ROUTE",
    "DROP_RULE",
    "DROP_SCHEMA",
    "DROP_SEARCH_PROPERTY_LIST",
    "DROP_SECURITY_POLICY",
    "DROP_SENSITIVITY_CLASSIFICATION",
    "DROP_SEQUENCE",
    "DROP_SERVICE",
    "DROP_SIGNATURE",
    "DROP_SIGNATURE_SCHEMA_OBJECT",
    "DROP_STATISTICS",
    "DROP_SYMMETRIC_KEY",
    "DROP_SYNONYM",
    "DROP_TABLE",
    "DROP_TRIGGER",
    "DROP_TYPE",
    "DROP_USER",
    "DROP_VIEW",
    "DROP_XML_SCHEMA_COLLECTION",
    "GRANT_DATABASE",
    "RENAME",
    "REVOKE_DATABASE",
    "UNBIND_DEFAULT",
    "UNBIND_RULE",
    "UNDO_DROP",
    "UPDATE_STATISTICS",
];
