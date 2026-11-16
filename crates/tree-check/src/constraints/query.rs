use super::Violation;
use super::util::*;
use crate::tree::{ancestors, has_child, named_children, preorder};
use tree_sitter::Node;

// Local statement constraints measured by query-forms on LocalDB 17.0.1000.7, levels 100-170.
// Keep the engine's surprising acceptances: repeated identical cursor flags, DATEFIRST=8 and
// DATEFORMAT=zzz pass PARSEONLY, so none is rejected here.
// offsets-bogus: Msg 195; offsets-valid/repeat: accepted (same engine and levels as above).
pub(super) fn query_offsets(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let parts: Vec<Node> = named_children(node);
    if !parts
        .get(1)
        .is_some_and(|n| n.kind() == "object_reference" && upper_text(*n, source) == "OFFSETS")
    {
        return;
    }
    for part in parts.iter().skip(2) {
        if matches!(part.kind(), "keyword_on" | "keyword_off") {
            continue;
        }
        if ![
            "SELECT",
            "FROM",
            "ORDER",
            "COMPUTE",
            "TABLE",
            "PROCEDURE",
            "EXECUTE",
            "STATEMENT",
            "PARAM",
        ]
        .contains(&upper_text(*part, source).as_str())
        {
            out.push(violation(
                "query-offsets-options",
                "unknown OFFSETS statement type".into(),
                *part,
            ));
        }
    }
}

pub(super) fn query_cursor(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let children: Vec<Node> = named_children(node);
    let Some(at) = children.iter().position(|n| n.kind() == "keyword_cursor") else {
        return;
    };
    let before: Vec<String> = children[..at]
        .iter()
        .map(|n| upper_text(*n, source))
        .collect();
    let mut flags = before
        .iter()
        .filter(|s| matches!(s.as_str(), "INSENSITIVE" | "SCROLL"))
        .cloned()
        .collect::<Vec<_>>();
    for n in &children[at + 1..] {
        if n.kind() == "keyword_for" {
            break;
        }
        flags.push(upper_text(*n, source));
    }
    let groups = [
        &["LOCAL", "GLOBAL"][..],
        &["FORWARD_ONLY", "SCROLL"][..],
        &["STATIC", "KEYSET", "DYNAMIC", "FAST_FORWARD"][..],
        &["READ_ONLY", "SCROLL_LOCKS", "OPTIMISTIC"][..],
    ];
    let known = [
        "LOCAL",
        "GLOBAL",
        "FORWARD_ONLY",
        "SCROLL",
        "STATIC",
        "KEYSET",
        "DYNAMIC",
        "FAST_FORWARD",
        "READ_ONLY",
        "SCROLL_LOCKS",
        "OPTIMISTIC",
        "TYPE_WARNING",
    ];
    let for_update =
        preorder(node).any(|part| part.kind() == "for_clause" && has_child(part, "keyword_update"));
    let invalid = flags
        .iter()
        .any(|f| !known.contains(&f.as_str()) && f != "INSENSITIVE")
        || children[at + 1..]
            .iter()
            .take_while(|n| n.kind() != "keyword_for")
            .any(|n| upper_text(*n, source) == "INSENSITIVE")
        || groups
            .iter()
            .any(|g| g.iter().filter(|v| flags.iter().any(|f| f == **v)).count() > 1)
        || flags.iter().any(|f| f == "FAST_FORWARD") && flags.iter().any(|f| f == "SCROLL")
        || flags.iter().any(|f| f == "INSENSITIVE") && for_update;
    // cursor-bogus/iso-after: 155/153; conflicting flags and ISO FOR UPDATE: 1048.
    if invalid {
        out.push(violation(
            "query-cursor-options",
            "unsupported or conflicting cursor options".into(),
            node,
        ));
    }
}

pub(super) fn query_atomic(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let options: Vec<Node> = named_children(node)
        .into_iter()
        .filter(|n| n.kind() == "atomic_option")
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut invalid = false;
    for option in options {
        let mut oc = option.walk();
        let parts: Vec<Node> = option.syntax_children(&mut oc).collect();
        let isolation = parts
            .first()
            .is_some_and(|n| n.kind() == "keyword_transaction");
        let item = if isolation {
            option
        } else {
            option.syntax_named_child(0).unwrap_or(option)
        };
        let key = if isolation {
            "ISOLATION".into()
        } else {
            item.child_by_field_name("name")
                .map(|n| upper_text(n, source))
                .unwrap_or_default()
        };
        invalid |= !seen.insert(key.clone());
        if isolation {
            continue;
        }
        let Some(value) = item.child_by_field_name("value") else {
            invalid = true;
            continue;
        };
        invalid |= !has_child(item, "=");
        invalid |= match key.as_str() {
            "LANGUAGE" => {
                value.kind() != "literal"
                    || !upper_text(value, source)
                        .trim_start_matches('N')
                        .starts_with('\'')
            }
            "DATEFIRST" => {
                value.kind() != "literal"
                    || !upper_text(value, source)
                        .bytes()
                        .all(|b| b.is_ascii_digit())
            }
            "DATEFORMAT" => !matches!(value.kind(), "identifier" | "literal"),
            "DELAYED_DURABILITY" => !matches!(value.kind(), "keyword_on" | "keyword_off"),
            _ => true,
        };
    }
    invalid |= !seen.iter().any(|k| k == "ISOLATION") || !seen.iter().any(|k| k == "LANGUAGE");
    let module = ancestors(node).any(|p| {
        matches!(
            p.kind(),
            "create_procedure"
                | "alter_procedure"
                | "create_function"
                | "alter_function"
                | "create_trigger"
                | "alter_trigger"
        )
    });
    // atomic-no-*, repeat-*, unknown and malformed values: 10784/1039/195/102; plain-atomic: 102.
    if invalid || !module {
        out.push(violation(
            "query-atomic-options",
            "ATOMIC needs a module and unique, complete supported options".into(),
            node,
        ));
    }
}

pub(super) fn query_module(node: Node, out: &mut Vec<Violation>) {
    let mut native = false;
    let mut atomic = false;
    let mut returns_null = false;
    for part in preorder(node) {
        native |= part.kind() == "keyword_native_compilation";
        atomic |= part.kind() == "atomic_clause";
        returns_null |= part.kind() == "function_option"
            && part
                .syntax_named_child(0)
                .is_some_and(|n| n.kind() == "keyword_returns");
    }
    // function-native-returns-null and kit B04-r052/r089: 10794.
    if native && returns_null {
        out.push(violation(
            "query-native-null-input",
            "native functions require CALLED ON NULL INPUT".into(),
            node,
        ));
    }
    // function-native-missing-atomic: 10783.
    if native && !atomic {
        out.push(violation(
            "query-native-atomic",
            "NATIVE_COMPILATION needs BEGIN ATOMIC".into(),
            node,
        ));
    }
}

// select-distinct-all: 156. Preserve quantified comparisons elsewhere in expressions.
pub(super) fn query_select(node: Node, out: &mut Vec<Violation>) {
    if !named_children(node)
        .into_iter()
        .any(|n| n.kind() == "keyword_distinct")
    {
        return;
    }
    let first = named_children(node)
        .into_iter()
        .find(|n| n.kind() == "select_expression")
        .and_then(|n| n.syntax_named_child(0))
        .and_then(|n| n.child_by_field_name("value"));
    if first
        .and_then(|n| n.child_by_field_name("operator"))
        .is_some_and(|n| n.kind() == "keyword_all")
    {
        out.push(violation(
            "query-select-quantifier",
            "DISTINCT and ALL are mutually exclusive".into(),
            node,
        ));
    }
}

// function-cursor-param: 10725/178; procedure-cursor-param is accepted.
pub(super) fn query_parameter(node: Node, out: &mut Vec<Violation>) {
    if !named_children(node)
        .into_iter()
        .any(|n| n.kind() == "keyword_cursor")
    {
        return;
    }
    for p in ancestors(node) {
        match p.kind() {
            "create_function" | "alter_function" => {
                out.push(violation(
                    "query-cursor-parameter",
                    "CURSOR parameters belong to procedures".into(),
                    node,
                ));
                return;
            }
            "create_procedure" | "alter_procedure" => return,
            _ => {}
        }
    }
}

pub(super) fn query_cast(node: Node, out: &mut Vec<Violation>) {
    let parts: Vec<Node> = named_children(node);
    // cast-using/try-cast-using: 102. PARSE and TRY_PARSE accept USING.
    if parts.iter().any(|n| n.kind() == "keyword_using")
        && !parts
            .iter()
            .any(|n| matches!(n.kind(), "keyword_parse" | "keyword_try_parse"))
    {
        out.push(violation(
            "query-cast-using",
            "USING belongs to PARSE or TRY_PARSE".into(),
            node,
        ));
    }
}

pub(super) fn query_exec(node: Node, out: &mut Vec<Violation>) {
    let mut recompile = false;
    let mut results = false;
    for part in named_children(node).into_iter() {
        let repeated = match part.kind() {
            "keyword_recompile" => std::mem::replace(&mut recompile, true),
            "exec_result_sets" => std::mem::replace(&mut results, true),
            _ => false,
        };
        // exec-repeat-recompile/results: 102.
        if repeated {
            out.push(violation(
                "query-exec-options",
                "EXEC options occur at most once".into(),
                part,
            ));
        }
    }
}

pub(super) fn query_rowset_schema(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let relation = node.syntax_prev_named_sibling();
    let call = relation.and_then(|r| {
        if r.kind() == "invocation" {
            Some(r)
        } else {
            r.syntax_named_child(0)
        }
    });
    let name = call
        .filter(|n| n.kind() == "invocation")
        .and_then(|n| n.syntax_named_child(0))
        .map(|n| upper_text(n, source))
        .unwrap_or_default();
    // rowset-56 and expr-forms/provider/schema: provider credentials take no WITH schema (102).
    if name == "OPENROWSET"
        && call.is_some_and(|call| {
            let mut c = call.walk();
            call.children_by_field_name("parameter", &mut c)
                .any(|arg| has_child(arg, ";"))
        })
    {
        out.push(violation(
            "expression-rowset-provider",
            "OPENROWSET provider credentials take no WITH rowset schema".into(),
            node,
        ));
        return;
    }
    // R20 gap-forms/rowset: only these built-in rowsets take a WITH column schema.
    if !matches!(
        name.as_str(),
        "OPENJSON" | "OPENXML" | "OPENROWSET" | "PREDICT"
    ) {
        out.push(violation(
            "query-rowset-schema",
            "this source takes no WITH column schema".into(),
            node,
        ));
        return;
    }
    if node.kind() == "openxml_table_schema" {
        if name != "OPENXML" {
            out.push(violation(
                "query-rowset-schema",
                "a table schema reference belongs to OPENXML".into(),
                node,
            ));
        }
        return;
    }
    for column in named_children(node)
        .into_iter()
        .filter(|n| n.kind() == "openjson_column")
    {
        // predict-schema accepted; OPENJSON/OPENXML nullability: 156/319.
        if (name != "PREDICT"
            && named_children(column)
                .into_iter()
                .any(|n| matches!(n.kind(), "keyword_null" | "keyword_not" | "keyword_collate")))
            || (name != "OPENJSON" && has_child(column, "keyword_json"))
            || column.child_by_field_name("path").is_some_and(|p| {
                if name == "PREDICT" {
                    string_literal(p, source)
                } else if name == "OPENXML" || name == "OPENJSON" {
                    !string_literal(p, source)
                } else {
                    false
                }
            })
        {
            out.push(violation(
                "query-rowset-schema",
                "rowset column path, AS JSON and nullability must match the provider".into(),
                column,
            ));
            break;
        }
    }
}

pub(super) fn query_dml_source(node: Node, context: &Context, out: &mut Vec<Violation>) {
    if context.parent().is_none_or(|n| n.kind() != "relation") {
        return;
    }
    let output = named_children(node)
        .into_iter()
        .any(|n| n.kind() == "output_clause");
    let insert = context.ancestor("insert").is_some();
    // insert-delete-no-output: 10716; select-delete-source: 156.
    if !output || !insert {
        out.push(violation(
            "query-dml-source",
            "a DML row source needs OUTPUT and an enclosing INSERT".into(),
            node,
        ));
    }
}

pub(super) fn query_assignment(node: Node, source: &[u8], out: &mut Vec<Violation>) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    let comparison = right.kind() == "binary_expression"
        && right
            .child_by_field_name("operator")
            .is_some_and(|n| n.kind() == "=");
    // update-column-chain: 102; update-variable-chain/compound-chain accepted.
    if (right.kind() == "assignment" || comparison) && !upper_text(left, source).starts_with('@') {
        out.push(violation(
            "query-assignment-chain",
            "only a variable can capture a column assignment".into(),
            node,
        ));
    }
}
