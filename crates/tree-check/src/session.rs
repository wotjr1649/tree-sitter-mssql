//! Spell OFF double-quoted strings as single-quoted strings for internal validation only.
use crate::{
    QuotedIdentifier,
    tree::{ancestors, preorder},
};
use std::ops::Range;
use tree_sitter::Tree;

fn space_len(source: &[u8]) -> usize {
    match source {
        [0..=32, ..] => 1,
        [0xc2, 0x85 | 0xa0, ..] => 2,
        [0xe1, 0x9a, 0x80, ..]
        | [0xe2, 0x80, 0x80..=0x8b | 0xa8 | 0xa9 | 0xaf, ..]
        | [0xe2, 0x81, 0x9f, ..]
        | [0xe3, 0x80, 0x80, ..] => 3,
        _ => 0,
    }
}

pub fn module_ranges(tree: &Tree) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for node in preorder(tree.root_node()) {
        if matches!(
            node.kind(),
            "create_procedure"
                | "alter_procedure"
                | "create_function"
                | "alter_function"
                | "create_trigger"
                | "alter_trigger"
        ) {
            let end = ancestors(node)
                .find(|n| n.kind() == "batch")
                .map_or(node.end_byte(), |n| n.end_byte());
            ranges.push(node.start_byte()..end);
        }
    }
    ranges
}

pub fn project(
    source: &[u8],
    initial: QuotedIdentifier,
    modules: &[Range<usize>],
) -> (Vec<u8>, Vec<usize>) {
    let mut out = Vec::with_capacity(source.len());
    let mut origins = Vec::with_capacity(source.len() + 1);
    let mut setting = initial;
    let mut words: Vec<Vec<u8>> = Vec::new();
    let mut at = 0;
    while at < source.len() {
        let start = at;
        if source[at] == b':'
            && source[..at]
                .rsplit(|b| *b == b'\n')
                .next()
                .unwrap()
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            // SQLCMD client lines are not parsed under the server session's setting.
            while at < source.len() && source[at] != b'\n' {
                at += 1;
            }
            words.clear();
        } else if source[at..].starts_with(b"--") {
            while at < source.len() && source[at] != b'\n' {
                at += 1;
            }
        } else if source[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1;
            while at < source.len() && depth > 0 {
                if source[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if source[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
        } else if matches!(source[at], b'\'' | b'"' | b'[') {
            let quote = source[at];
            let close = if quote == b'[' { b']' } else { quote };
            at += 1;
            let mut closed = false;
            while at < source.len() {
                if source[at] == close {
                    at += 1;
                    if source.get(at) != Some(&close) {
                        closed = true;
                        break;
                    }
                }
                at += 1;
            }
            words.clear();
            if quote == b'"' && closed && setting == QuotedIdentifier::Off {
                out.push(b'\'');
                origins.push(start);
                let mut index = start + 1;
                while index < at - 1 {
                    let byte = source[index];
                    out.push(byte);
                    origins.push(index);
                    if byte == b'\'' {
                        out.push(byte);
                        origins.push(index);
                    }
                    if byte == b'"' {
                        index += 1;
                    }
                    index += 1;
                }
                out.push(b'\'');
                origins.push(at - 1);
                continue;
            }
        } else if source[at].is_ascii_alphanumeric()
            || matches!(source[at], b'_' | b'@' | b'#' | b'$')
        {
            while at < source.len()
                && (source[at].is_ascii_alphanumeric()
                    || matches!(source[at], b'_' | b'@' | b'#' | b'$'))
            {
                at += 1;
            }
            let word = source[start..at].to_ascii_uppercase();
            if !modules.iter().any(|range| range.contains(&start))
                && words.len() >= 2
                && words[words.len() - 2] == b"SET"
                && words[words.len() - 1] == b"QUOTED_IDENTIFIER"
            {
                match word.as_slice() {
                    b"ON" => setting = QuotedIdentifier::On,
                    b"OFF" => setting = QuotedIdentifier::Off,
                    _ => {}
                }
            }
            words.push(word);
            if words.len() > 2 {
                words.remove(0);
            }
        } else {
            let space = space_len(&source[at..]);
            if space == 0 {
                words.clear();
            }
            at += space.max(1);
        }
        out.extend_from_slice(&source[start..at]);
        origins.extend(start..at);
    }
    origins.push(source.len());
    (out, origins)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_comments_unicode_and_switches_keep_origins() {
        let sql = b"SELECT \"a'\"\"b\"; /* SET QUOTED_IDENTIFIER ON */ SET /*x*/ QUOTED_IDENTIFIER ON; SELECT \"name\";";
        let (out, origins) = project(sql, QuotedIdentifier::Off, &[]);
        assert_eq!(out, b"SELECT 'a''\"b'; /* SET QUOTED_IDENTIFIER ON */ SET /*x*/ QUOTED_IDENTIFIER ON; SELECT \"name\";");
        assert_eq!(origins.len(), out.len() + 1);
        for (i, byte) in out.iter().enumerate() {
            assert!(*byte == sql[origins[i]] || *byte == b'\'' && sql[origins[i]] == b'"');
        }
        assert_eq!(
            project(b"SELECT \"open", QuotedIdentifier::Off, &[]).0,
            b"SELECT \"open"
        );
        assert_eq!(
            project(b"SELECT \"\";", QuotedIdentifier::Off, &[]).0,
            b"SELECT '';"
        );
        assert_eq!(
            project(
                b":r \"SET QUOTED_IDENTIFIER ON\"\nSELECT \"value\";",
                QuotedIdentifier::Off,
                &[]
            )
            .0,
            b":r \"SET QUOTED_IDENTIFIER ON\"\nSELECT 'value';"
        );
    }

    #[test]
    fn engine_whitespace_separates_the_session_words() {
        for character in (0..=32)
            .chain([0x85, 0xa0, 0x1680])
            .chain(0x2000..=0x200b)
            .chain([0x2028, 0x2029, 0x202f, 0x205f, 0x3000])
        {
            let space = char::from_u32(character).unwrap();
            let sql = format!("SET{space}QUOTED_IDENTIFIER{space}OFF; SELECT \"value\";");
            let expected = format!("SET{space}QUOTED_IDENTIFIER{space}OFF; SELECT 'value';");
            assert_eq!(
                project(sql.as_bytes(), QuotedIdentifier::On, &[]).0,
                expected.as_bytes(),
                "U+{character:04X}"
            );
        }
        assert_eq!(space_len("\u{feff}".as_bytes()), 0);
    }
}
