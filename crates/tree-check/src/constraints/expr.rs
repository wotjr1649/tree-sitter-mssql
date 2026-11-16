use super::Violation;
use super::util::*;
use crate::tree::{ancestors, has_child, named_children};
use tree_sitter::Node;

/// R18 last-forms/escape: ESCAPE follows an unparenthesized LIKE/NOT LIKE predicate (156).
/// R20 gap-forms/escape also rejects a search condition used as the escape character (102/156).
pub(super) fn like_escape(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    if node
        .child_by_field_name("operator")
        .is_none_or(|n| n.kind() != "keyword_escape")
    {
        return;
    }
    let valid = node.child_by_field_name("left").is_some_and(|n| {
        n.kind() == "binary_expression"
            && n.child_by_field_name("operator")
                .is_some_and(|op| matches!(op.kind(), "keyword_like" | "not_like"))
    });
    let scalar = node
        .child_by_field_name("right")
        .is_none_or(|n| !is_condition(n, source));
    if !valid || !scalar {
        out.push(violation(
            "expression-like-escape",
            "ESCAPE must directly follow LIKE or NOT LIKE and take a scalar".into(),
            node,
        ));
    }
}

/// R18 last-forms/json*: argument count/shape, NULL/ORDER/RETURNING and window boundaries
/// measured on LocalDB 17.0.1000.7, levels 100-170 (102/156/174/10759).
pub(super) fn json_aggregate(
    node: Node,
    context: &Context,
    source: &[u8],
    out: &mut Vec<Violation>,
) {
    let Some(callee) = node.syntax_named_child(0) else {
        return;
    };
    let name = upper_text(callee, source);
    let array = name == "JSON_ARRAYAGG";
    let object = name == "JSON_OBJECTAGG";
    let mut c = node.walk();
    let args: Vec<_> = node.children_by_field_name("parameter", &mut c).collect();
    let pair = args.iter().any(|a| a.kind() == "json_key_value");
    let argument_end = node
        .syntax_children(&mut node.walk())
        .find(|n| n.kind() == ")")
        .map_or(node.end_byte(), |n| n.start_byte());
    let order = named_children(node)
        .into_iter()
        .any(|n| n.kind() == "order_by" && n.start_byte() < argument_end);
    let nulls = has_child(node, "json_null_clause");
    if !array && !object {
        if pair || order || nulls {
            out.push(violation(
                "expression-json-aggregate",
                "JSON pairs, NULL handling and argument ORDER BY require a JSON aggregate".into(),
                node,
            ));
        }
        return;
    }
    let within = has_child(node, "keyword_within");
    let distinct = has_child(node, "keyword_distinct");
    let window = context
        .parent()
        .is_some_and(|p| p.kind() == "window_function");
    let returning = named_children(node)
        .into_iter()
        .find(|n| n.kind() == "returning_clause");
    let wrong_return = returning.is_some_and(|n| {
        named_children(n)
            .into_iter()
            .nth(1)
            .is_none_or(|ty| upper_text(ty, source).trim_matches(['[', ']', '"']) != "JSON")
    });
    let wrong_arg = args.len() != 1
        || args.iter().any(|arg| {
            if object {
                arg.kind() != "json_key_value"
            } else {
                arg.kind() != "term"
                    || arg.child_by_field_name("alias").is_some()
                    || arg
                        .child_by_field_name("value")
                        .is_none_or(|v| v.kind() == "all_fields")
            }
        });
    if wrong_arg
        || wrong_return
        || has_child(node, "json_wrapper_clause")
        || (object && (order || distinct || within))
        || (array && distinct && (order || nulls || returning.is_some() || window))
        || (array && order && window)
        || (array && within && (order || nulls || returning.is_some() || distinct))
    {
        out.push(violation("expression-json-aggregate", "JSON aggregates require one value or key:value, measured NULL/ORDER/RETURNING clauses and window form".into(), node));
    }
}

/// R18 last-forms/into: any set branch may have INTO, even several branches; a nested
/// query, INSERT source or view body cannot (LocalDB 17.0.1000.7, levels 100-170, 156).
pub(super) fn select_into_scope(node: Node, out: &mut Vec<Violation>) {
    if !has_child(node, "keyword_into") {
        return;
    }
    for n in ancestors(node) {
        if matches!(
            n.kind(),
            "subquery" | "cte" | "insert" | "create_view" | "alter_view"
        ) {
            out.push(violation(
                "query-select-into-scope",
                "INTO is not allowed in a subquery, CTE, INSERT source or view".into(),
                node,
            ));
            return;
        }
    }
}

/// R15: LocalDB 17.0.1000.7, levels 100-170, expr-forms/language.
/// Keep argument positions and value restrictions here instead of multiplying grammar states.
pub(super) fn expression_language(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(callee) = node.syntax_named_child(0) else {
        return;
    };
    let name = upper_text(callee, source);
    let mut cursor = node.walk();
    let args: Vec<_> = node
        .children_by_field_name("parameter", &mut cursor)
        .collect();
    let language: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| {
            arg.child_by_field_name("value")
                .is_some_and(|v| v.kind() == "field" && upper_text(v, source) == "LANGUAGE")
                && (arg.child_by_field_name("alias").is_some()
                    || matches!(
                        name.as_str(),
                        "CONTAINS" | "FREETEXT" | "CONTAINSTABLE" | "FREETEXTTABLE"
                    ))
        })
        .collect();
    if !language.is_empty() {
        let table = matches!(name.as_str(), "CONTAINSTABLE" | "FREETEXTTABLE");
        let expected = if table { 3 } else { 2 };
        let valid_name = table || matches!(name.as_str(), "CONTAINS" | "FREETEXT");
        let mut invalid = !valid_name
            || language.len() != 1
            || language[0].0 != expected
            || args.len() < expected + 1
            || args.len() > expected + 1 + usize::from(table);
        for (_, arg) in &language {
            invalid |= !arg.child_by_field_name("alias").is_some_and(|value| {
                let text = &source[value.byte_range()];
                string_literal(value, source)
                    || integer_literal(value, source)
                    || local_variable(value, source)
                    || (text.first() == Some(&b'"') && quoted_strings(node, source))
            });
        }
        if table && args.len() == expected + 2 {
            let top = args[expected + 1];
            invalid |= top.child_by_field_name("alias").is_some()
                || !top.child_by_field_name("value").is_some_and(|v| {
                    local_variable(v, source)
                        || (v.kind() == "literal"
                            && source[v.byte_range()].iter().all(u8::is_ascii_digit))
                });
        }
        if invalid {
            out.push(violation("expression-fulltext-language", "LANGUAGE needs a full-text function, its language argument position and a language name, LCID or local variable".into(), node));
        }
    }
}

/// R15 expr-forms/provider: credentials are confined to OPENROWSET's second argument.
pub(super) fn expression_provider(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(callee) = node.syntax_named_child(0) else {
        return;
    };
    let name = upper_text(callee, source);
    let mut cursor = node.walk();
    let args: Vec<_> = node
        .children_by_field_name("parameter", &mut cursor)
        .collect();
    let connections: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| has_child(**arg, ";"))
        .collect();
    if !connections.is_empty() {
        let mut invalid = name != "OPENROWSET"
            || connections.len() != 1
            || connections[0].0 != 1
            || args.len() != 3;
        for (_, connection) in &connections {
            invalid |= named_children(**connection)
                .into_iter()
                .any(|n| !string_literal(n, source));
        }
        invalid |= !args
            .first()
            .and_then(|n| n.child_by_field_name("value"))
            .is_some_and(|n| string_literal(n, source));
        if let Some(query) = args.get(2) {
            invalid |= query.child_by_field_name("alias").is_some()
                || !query.child_by_field_name("value").is_some_and(|v| {
                    string_literal(v, source)
                        || (v.kind() == "field" && !upper_text(v, source).starts_with('@'))
                });
        }
        if invalid {
            out.push(violation("expression-rowset-provider", "OPENROWSET takes provider, server;[user];[password], query or table; connection components are strings".into(), node));
        }
    }
}

/// R15 expr-forms/set: an operand cannot carry its own trailing clauses, even with TOP.
pub(super) fn nested_set_clauses(node: Node, context: &Context, out: &mut Vec<Violation>) {
    if context
        .parent()
        .is_some_and(|n| n.kind() == "query_specification")
        && named_children(node).into_iter().any(|n| {
            matches!(
                n.kind(),
                "order_by" | "offset_fetch" | "option_clause" | "for_clause"
            )
        })
    {
        out.push(violation(
            "expression-nested-set-clause",
            "a nested set operand has no trailing sort, hint or FOR clause".into(),
            node,
        ));
    }
}

/// R15 expr-forms/static: type names have at most two parts; variables are not type/member names.
pub(super) fn static_reference(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    if !has_child(node, "::") {
        return;
    }
    let Some(ty) = node.child_by_field_name("schema") else {
        return;
    };
    let Some(member) = node.child_by_field_name("name") else {
        return;
    };
    let mut c = ty.walk();
    let mut invalid = ty
        .syntax_children(&mut c)
        .filter(|n| n.kind() == ".")
        .count()
        > 1;
    for part in [ty, member] {
        invalid |= named_children(part)
            .into_iter()
            .any(|n| source[n.byte_range()].starts_with(b"@") || upper_text(n, source) == "NULL");
    }
    if invalid {
        out.push(violation(
            "expression-static-reference",
            "a static type has at most schema.name and no variable type or member".into(),
            node,
        ));
    }
}

/// Built-in functions with a fixed number of arguments: the one-argument aggregates (`*` counts as
/// the argument of COUNT and COUNT_BIG), ABS, LEN, and the two-argument ISNULL and STRING_AGG. A
/// schema-qualified or bracketed name is not checked. Evidence: SQL Server 2025 LocalDB 17.0.1000.7,
/// levels 100-170, oracle sets `context/count-*`, `context/arity-*` and the count-* cases: Msg 174
/// for any other count (Msg 102 for some shapes); accepted otherwise.
pub(super) fn builtin_arity(invocation: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(name) = invocation
        .syntax_named_child(0)
        .filter(|n| n.kind() == "object_reference" && n.syntax_named_child_count() == 1)
    else {
        return;
    };
    let name = upper_text(name, source);
    let expected = match name.as_str() {
        "COUNT"
        | "COUNT_BIG"
        | "AVG"
        | "MIN"
        | "MAX"
        | "SUM"
        | "STDEV"
        | "STDEVP"
        | "VAR"
        | "VARP"
        | "CHECKSUM_AGG"
        | "GROUPING"
        | "APPROX_COUNT_DISTINCT"
        | "ABS"
        | "LEN" => 1,
        "ISNULL" | "STRING_AGG" => 2,
        _ => return,
    };
    let mut c = invocation.walk();
    let given = invocation
        .children_by_field_name("parameter", &mut c)
        .count();
    if given != expected {
        out.push(violation(
            "builtin-arity",
            format!("{name} takes {expected} argument(s), not {given}"),
            invocation,
        ));
    }
}

/// A scalar function (RETURNS a type, not TABLE) must RETURN a value everywhere in its body.
/// Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set `context/return-*`
/// and the scalar-return-* cases: Msg 1075 for a bare RETURN at any depth, in CREATE and ALTER;
/// accepted in multi-statement table functions and procedures.
pub(super) fn scalar_function_return(function: Node, out: &mut Vec<Violation>) {
    let mut cursor = function.walk();
    if function
        .syntax_children(&mut cursor)
        .any(|n| matches!(n.kind(), "keyword_table" | "table_var_declaration"))
    {
        return;
    }
    let Some(body) = function.child_by_field_name("body") else {
        return;
    };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if n.kind() == "return_statement" && n.syntax_named_child_count() == 1 {
            out.push(violation(
                "scalar-function-return",
                "RETURN in a scalar function needs a value".into(),
                n,
            ));
        }
        stack.extend(named_children(n));
    }
}

/// A window frame (ROWS/RANGE) needs ORDER BY in the same OVER clause or a named base window.
/// Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170, oracle set `context/window-*` and
/// window-no-order-*: Msg 10756 (Msg 102 for some ROWS shapes) without ORDER BY, also when ORDER BY
/// only sits in a nested query; accepted with ORDER BY and, at 160-170, with a base window.
pub(super) fn window_frame_order(spec: Node, out: &mut Vec<Violation>) {
    let children: Vec<Node> = named_children(spec);
    let Some(frame) = children.iter().find(|n| n.kind() == "window_frame") else {
        return;
    };
    if spec.child_by_field_name("base_window").is_none()
        && !children.iter().any(|n| n.kind() == "order_by")
    {
        out.push(violation(
            "window-frame-order",
            "a window frame needs ORDER BY".into(),
            *frame,
        ));
    }
}

/// A position that needs a search condition holds a scalar expression (Msg 4145, "An expression of
/// non-boolean type specified in a context where a condition is expected"). The positions are every
/// `predicate` field (WHERE, JOIN ... ON, MERGE ... ON, WHEN [NOT] MATCHED ... AND), the condition of IF
/// and WHILE, the WHEN of a searched CASE (not a simple CASE, which has an `input`), the HAVING
/// expression and the first argument of IIF. Ported from grammar-kit's CheckTSQLContext (ADR-0004).
/// Evidence: SQL Server 2025 LocalDB 17.0.1000.7, levels 100-170: Msg 4145 for kit if-bit,
/// if-not-scalar, iif-scalar, merge-arith, merge-paren-scalar, merge-scalar and mut-0028x; every
/// engine-accepted oracle case passes.
pub(super) fn scalar_predicate(
    node: Node,
    context: &Context,
    source: &[u8],
    out: &mut Vec<Violation>,
) {
    if node.child_count() == 0 {
        return;
    }
    let mut cursor = node.walk();
    // An Extended Events WHERE shares the rule but takes event predicates, not search conditions.
    let in_event_session = || {
        context.ancestor("create_event_session").is_some()
            || context.ancestor("alter_event_session").is_some()
    };
    let checked = node
        .children_by_field_name("predicate", &mut cursor)
        .filter(|_| node.kind() != "where" || !in_event_session());
    let extra = match node.kind() {
        "if_statement" | "while_statement" => node.child_by_field_name("condition"),
        "when_clause" => {
            let searched = context
                .parent()
                .is_some_and(|case| case.child_by_field_name("input").is_none());
            searched
                .then(|| node.child_by_field_name("condition"))
                .flatten()
        }
        "having" => node
            .syntax_named_children(&mut node.walk())
            .find(|n| n.kind() != "keyword_having"),
        "invocation" if invocation_name(node, source).as_deref() == Some("IIF") => {
            node.child_by_field_name("parameter")
        }
        _ => None,
    };
    for n in checked.chain(extra) {
        if !is_condition(n, source) {
            out.push(violation(
                "scalar-predicate",
                "a condition is expected here, not a scalar expression".into(),
                n,
            ));
        }
    }
}

/// The unqualified, upper-cased name of a function call, if it has one.
pub(super) fn invocation_name(invocation: Node, source: &[u8]) -> Option<String> {
    invocation
        .syntax_named_child(0)
        .filter(|n| n.kind() == "object_reference" && n.syntax_named_child_count() == 1)
        .map(|n| upper_text(n, source))
}

/// Whether an expression is a search condition (true/false/unknown) rather than a scalar value.
pub(super) fn is_condition(n: Node, source: &[u8]) -> bool {
    let mut pending = vec![n];
    while let Some(n) = pending.pop() {
        #[cfg(test)]
        crate::tree::record_visit();
        match n.kind() {
            "exists" | "graph_match_predicate" | "between_expression" => {}
            "invocation" => {
                if !matches!(
                    invocation_name(n, source).as_deref(),
                    Some("UPDATE" | "CONTAINS" | "FREETEXT" | "REGEXP_LIKE")
                ) {
                    return false;
                }
            }
            "term" | "parenthesized_expression" => {
                let Some(child) = n
                    .child_by_field_name("value")
                    .or_else(|| n.syntax_named_child(0))
                else {
                    return false;
                };
                pending.push(child);
            }
            "binary_expression" => {
                let operator = n.child_by_field_name("operator").map(|o| o.kind());
                match operator {
                    Some("keyword_and" | "keyword_or") => {
                        let (Some(left), Some(right)) = (
                            n.child_by_field_name("left"),
                            n.child_by_field_name("right"),
                        ) else {
                            return false;
                        };
                        pending.extend([left, right]);
                    }
                    Some(
                        "=" | "<" | "<=" | ">" | ">=" | "<>" | "!=" | "!<" | "!>" | "keyword_is"
                        | "is_not" | "keyword_like" | "not_like" | "keyword_in" | "not_in"
                        | "distinct_from" | "not_distinct_from" | "keyword_between"
                        | "keyword_escape",
                    ) => {}
                    _ => return false,
                }
            }
            "unary_expression" => {
                if n.child_by_field_name("operator").map(|o| o.kind()) != Some("keyword_not") {
                    return false;
                }
                let Some(operand) = n.child_by_field_name("operand") else {
                    return false;
                };
                pending.push(operand);
            }
            _ => return false,
        }
    }
    true
}
