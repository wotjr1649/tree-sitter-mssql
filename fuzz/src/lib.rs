//! Shared by libFuzzer and the stable, one-input-per-process driver.
use serde_json::Value;
use tree_check::{QuotedIdentifier, Report, edits::History};

fn report(report: &Report, source: &[u8]) {
    let value: Value = serde_json::from_str(&tree_check::json_line("fuzz", report)).unwrap();
    assert_eq!(value["len"].as_u64().unwrap(), source.len() as u64);
    assert!(value["root_end"].as_u64().unwrap() <= source.len() as u64);
    let problems: u64 = ["error_nodes", "missing_nodes", "zero_width_nodes"]
        .iter()
        .map(|key| value[key].as_u64().unwrap())
        .sum();
    assert_eq!(value["first_problem"].is_null(), problems == 0);
    let violations = value["constraint_violations"].as_array().unwrap();
    let ok = !value["root_has_error"].as_bool().unwrap()
        && problems == 0
        && value["root_end"] == value["len"]
        && violations.is_empty();
    assert_eq!(value["ok"].as_bool().unwrap(), ok);
    assert_eq!(report.ok(), ok);
    for diagnostic in violations
        .iter()
        .chain(std::iter::once(&value["first_problem"]).filter(|v| !v.is_null()))
    {
        let row = diagnostic["row"].as_u64().unwrap() as usize;
        let column = diagnostic["column"].as_u64().unwrap() as usize;
        let line = source.split(|byte| *byte == b'\n').nth(row).unwrap();
        assert!(column <= line.len());
    }
}

fn history(parser: &mut tree_sitter::Parser, history: &History) {
    if let Ok(differing) = tree_check::edits::check_history(parser, history) {
        assert!(
            differing
                .iter()
                .all(|step| (1..=history.edits.len()).contains(step))
        );
        assert!(differing.windows(2).all(|pair| pair[0] < pair[1]));
    }
}

pub fn check(source: &[u8]) {
    let mut parser = tree_check::grammar_parser().unwrap();
    let tree = tree_check::parse(&mut parser, source).unwrap();
    let default = tree_check::inspect(&tree, source);
    for setting in [QuotedIdentifier::On, QuotedIdentifier::Off] {
        let inspected = tree_check::inspect_with_quoted_identifier(&tree, source, setting);
        report(&inspected, source);
        if setting == QuotedIdentifier::On {
            assert_eq!(default, inspected);
        }
    }
    if let Ok(text) = std::str::from_utf8(source) {
        assert_eq!(
            serde_json::from_str::<String>(&tree_check::json_string(text)).unwrap(),
            text
        );
        history(
            &mut parser,
            &History {
                id: "fuzz".into(),
                source: text.into(),
                edits: vec![("".into(), "x".into()), ("x".into(), "".into())],
            },
        );
        if let Ok(histories) = tree_check::edits::read_histories(text) {
            for item in histories {
                history(&mut parser, &item);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn consumer_paths_accept_empty_invalid_utf8_quotes_and_histories() {
        for source in [
            b"".as_slice(),
            b"SELECT \xff;",
            b"SET QUOTED_IDENTIFIER OFF; SELECT \"a'b\";",
            br#"{"id":"x","source":"SELECT 1;","edits":[{"find":"1","replace":"2"}]}"#,
        ] {
            check(source);
        }
    }
}
