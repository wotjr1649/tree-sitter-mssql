//! Option-set constraints that SQL Server enforces while parsing but the grammar does not encode
//! (ADR-0003): an option at most once, required options, mutually exclusive options. Each rule
//! cites the engine evidence it was measured against.

use tree_sitter::{Node, Tree};

/// A constraint violation: the rule and the row/column (0-based) of the offending node.
#[derive(Debug, PartialEq)]
pub struct Violation {
    pub rule: &'static str,
    pub detail: String,
    pub row: usize,
    pub column: usize,
}

pub fn check(tree: &Tree, source: &[u8]) -> Vec<Violation> {
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if matches!(node.kind(), "create_queue" | "alter_queue") {
            queue_activation(node, source, &mut out);
        }
        if node.kind() == "statement" {
            cte_terminator(node, &mut out);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return out;
            }
        }
    }
}

/// A statement that starts with a CTE must not directly follow another statement or block: the
/// previous one needs a `;`. It may come first in a batch, block or module body, right after
/// `;`, a label, or as the body of IF, ELSE or WHILE. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, Msg 319 for the semicolon/*-no-terminator probes (after SELECT,
/// END, IF, IF ... ELSE, WHILE and inside a procedure body), acceptance of every terminated,
/// leading-semicolon, label, IF/ELSE/WHILE-body and first-in-block form.
fn cte_terminator(statement: Node, out: &mut Vec<Violation>) {
    // `WITH c AS (...) SELECT ...` is a statement whose children start with `keyword_with`, `cte`.
    let starts_with_cte = statement.named_child(0).map(|n| n.kind()) == Some("keyword_with")
        && statement.named_child(1).map(|n| n.kind()) == Some("cte");
    if !starts_with_cte {
        return;
    }
    let Some(prev) = statement.prev_sibling() else {
        return;
    };
    let follows_statement = match prev.kind() {
        "block" => true,
        "statement" => prev.named_child(0).map(|n| n.kind()) != Some("label_statement"),
        _ => false,
    };
    if follows_statement {
        out.push(violation(
            "cte-terminator",
            "the statement before a WITH common table expression must end with ';'".into(),
            statement,
        ));
    }
}

fn upper_text(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or_default()
        .to_ascii_uppercase()
}

fn violation(rule: &'static str, detail: String, node: Node) -> Violation {
    let at = node.start_position();
    Violation {
        rule,
        detail,
        row: at.row,
        column: at.column,
    }
}

/// CREATE/ALTER QUEUE ... WITH ACTIVATION (...): each setting at most once; CREATE also needs
/// PROCEDURE_NAME, MAX_QUEUE_READERS and EXECUTE AS. Evidence: SQL Server 2025 LocalDB
/// 17.0.1000.7, levels 100-170, Msg 102 for oracle cases queue-duplicate-create/alter and
/// queue-missing-48/49/50, acceptance of queue-missing-51.
fn queue_activation(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut cursor = statement.walk();
    let with_clauses = statement
        .children(&mut cursor)
        .filter(|n| n.kind() == "with_clause");
    for with_clause in with_clauses.collect::<Vec<_>>() {
        let mut c = with_clause.walk();
        for activation in with_clause.children(&mut c).filter(|n| {
            n.kind() == "option"
                && n.child_by_field_name("name")
                    .map(|name| upper_text(name, source))
                    == Some("ACTIVATION".into())
        }) {
            let mut seen: Vec<String> = Vec::new();
            let mut drop = false;
            let mut a = activation.walk();
            for setting in activation.named_children(&mut a) {
                let key = match setting.kind() {
                    "option" => match setting.child_by_field_name("name") {
                        Some(name) => upper_text(name, source),
                        None => continue,
                    },
                    "execute_as_clause" => "EXECUTE AS".into(),
                    "keyword_drop" => {
                        drop = true;
                        continue;
                    }
                    _ => continue, // the activation's own name
                };
                if seen.contains(&key) {
                    out.push(violation(
                        "queue-activation-duplicate",
                        format!("{key} appears more than once in ACTIVATION"),
                        setting,
                    ));
                } else {
                    seen.push(key);
                }
            }
            if statement.kind() == "create_queue" && !drop {
                for required in ["PROCEDURE_NAME", "MAX_QUEUE_READERS", "EXECUTE AS"] {
                    if !seen.iter().any(|k| k == required) {
                        out.push(violation(
                            "queue-activation-required",
                            format!("CREATE QUEUE ACTIVATION needs {required}"),
                            activation,
                        ));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn rules(sql: &str) -> Vec<&'static str> {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
        let tree = p.parse(sql, None).unwrap();
        check(&tree, sql.as_bytes())
            .into_iter()
            .map(|v| v.rule)
            .collect()
    }

    const FULL: &str = "PROCEDURE_NAME = dbo.p, MAX_QUEUE_READERS = 1, EXECUTE AS OWNER";

    #[test]
    fn complete_activation_in_any_order_passes() {
        assert!(rules(&format!("CREATE QUEUE q WITH ACTIVATION ({FULL});")).is_empty());
        let reordered = "EXECUTE AS SELF, STATUS = ON, MAX_QUEUE_READERS = 2, PROCEDURE_NAME = p";
        assert!(rules(&format!("CREATE QUEUE q WITH ACTIVATION ({reordered});")).is_empty());
    }

    #[test]
    fn create_needs_the_three_required_settings() {
        let missing = "CREATE QUEUE q WITH ACTIVATION (MAX_QUEUE_READERS = 1, EXECUTE AS OWNER, STATUS = OFF);";
        assert_eq!(rules(missing), ["queue-activation-required"]);
        assert_eq!(
            rules("CREATE QUEUE q WITH ACTIVATION (STATUS = ON);").len(),
            3
        );
    }

    #[test]
    fn alter_may_change_one_setting_or_drop() {
        assert!(rules("ALTER QUEUE q WITH ACTIVATION (STATUS = OFF);").is_empty());
        assert!(rules("ALTER QUEUE q WITH ACTIVATION (DROP);").is_empty());
    }

    #[test]
    fn a_repeated_setting_is_a_violation_in_create_and_alter() {
        let dup = format!("{FULL}, PROCEDURE_NAME = dbo.p");
        assert_eq!(
            rules(&format!("CREATE QUEUE q WITH ACTIVATION ({dup});")),
            ["queue-activation-duplicate"]
        );
        assert_eq!(
            rules(&format!("ALTER QUEUE q WITH ACTIVATION ({dup});")),
            ["queue-activation-duplicate"]
        );
        let exec_twice = "ALTER QUEUE q WITH ACTIVATION (EXECUTE AS SELF, EXECUTE AS OWNER);";
        assert_eq!(rules(exec_twice), ["queue-activation-duplicate"]);
    }

    const CTE: &str = "WITH c AS (SELECT 1 AS n) SELECT n FROM c;";

    #[test]
    fn a_cte_directly_after_a_statement_needs_a_terminator() {
        for before in [
            "SELECT 1
",
            "BEGIN SELECT 1; END
",
            "IF 1=1 SELECT 1
",
            "WHILE 1=0 SELECT 1
",
        ] {
            assert_eq!(
                rules(&format!("{before}{CTE}")),
                ["cte-terminator"],
                "{before}"
            );
        }
        assert_eq!(
            rules(&format!(
                "CREATE PROCEDURE dbo.p AS SELECT 1
{CTE}"
            )),
            ["cte-terminator"]
        );
    }

    #[test]
    fn a_terminated_leading_or_controlled_cte_passes() {
        for sql in [
            CTE.to_string(),
            format!("SELECT 1; {CTE}"),
            format!(";{CTE}"),
            format!("here: {CTE}"),
            format!("IF 1=1 {CTE}"),
            format!("WHILE 1=0 {CTE}"),
            format!("BEGIN {CTE} END;"),
            format!("BEGIN SELECT 1; END; {CTE}"),
            format!("CREATE PROCEDURE dbo.p AS {CTE}"),
        ] {
            assert!(rules(&sql).is_empty(), "{sql}");
        }
    }

    #[test]
    fn other_statements_are_not_checked() {
        assert!(rules("SELECT 1 WHERE 1 = 1;").is_empty());
    }
}
