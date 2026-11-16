use super::Violation;
use super::util::*;
use crate::tree::has_child;
use tree_sitter::Node;

/// BACKUP and RESTORE option names, shapes and values that SQL Server checks while parsing; the
/// grammar reads every name as an identifier. Evidence: LocalDB 17.0.1000.7, levels 100-170,
/// oracle/data/probes/backup-restore.jsonl: `opt-*` (every option name bare, `= 1` and `= 'x'` in
/// BACKUP and RESTORE: unknown or other-statement names Msg 155, a flag given a value or a value
/// option without one Msg 153), `device-*` (DISK, TAPE, URL, DATABASE_SNAPSHOT, VIRTUAL_DEVICE
/// accepted; FILE 156, PIPE 155), `value-*` (a string, an integer or a variable; identifiers,
/// NULL, hex and decimals Msg 102/156/1080), `compression-*`, `encryption-*`, `filestream-*`,
/// `move-*`, `mark-*` and `cert-*` (Msg 102). Repeated and conflicting options are accepted
/// (`backup-duplicate-*`, `backup-init-noinit`, `restore-recovery-norecovery`), so no uniqueness.
pub(super) fn backup_restore(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let restore = statement.kind() == "restore_statement";
    let Some(form) = statement.syntax_child(1) else {
        return;
    };
    let mut cursor = statement.walk();
    let children: Vec<Node> = statement.syntax_children(&mut cursor).collect();
    if form.kind() == "keyword_certificate" {
        backup_certificate(&children, source, out);
        return;
    }
    if has_child(statement, "keyword_key") {
        return; // MASTER, SERVICE MASTER and SYMMETRIC KEY: the grammar fixes their shape
    }
    // Options right after the name are files; after TO, MIRROR TO or FROM they are devices.
    let mut section = "";
    for node in children {
        match node.kind() {
            "identifier" if form.kind() == "keyword_group" => {
                if value_kind(node, source) == 'v' {
                    out.push(violation(
                        "backup-restore-device",
                        "BACKUP GROUP takes database names, not variables".into(),
                        node,
                    ));
                }
            }
            "identifier" if section.is_empty() => section = "file",
            "keyword_to" | "keyword_from" => section = "device",
            "option" if section == "file" => backup_file(node, form.kind(), source, out),
            "option" if section == "device" => backup_device(node, source, out),
            "with_clause" => {
                for option in direct_options(node) {
                    backup_option(option, restore, source, out);
                }
            }
            _ => {}
        }
    }
}

pub(super) const BACKUP_FLAGS: &[&str] = &[
    "DIFFERENTIAL",
    "COPY_ONLY",
    "COMPRESSION",
    "NO_COMPRESSION",
    "FILE_SNAPSHOT",
    "NOINIT",
    "INIT",
    "NOSKIP",
    "SKIP",
    "NOFORMAT",
    "FORMAT",
    "NO_CHECKSUM",
    "CHECKSUM",
    "STOP_ON_ERROR",
    "CONTINUE_AFTER_ERROR",
    "RESTART",
    "REWIND",
    "NOREWIND",
    "UNLOAD",
    "NOUNLOAD",
    "NORECOVERY",
    "NO_TRUNCATE",
    "METADATA_ONLY",
    "SNAPSHOT",
    "NO_LOG",
    "STATS",
];

pub(super) const BACKUP_VALUES: &[&str] = &[
    "STATS",
    "DESCRIPTION",
    "NAME",
    "CREDENTIAL",
    "EXPIREDATE",
    "RETAINDAYS",
    "MEDIADESCRIPTION",
    "MEDIANAME",
    "BLOCKSIZE",
    "BUFFERCOUNT",
    "MAXTRANSFERSIZE",
    "STANDBY",
    "PASSWORD",
    "MEDIAPASSWORD",
    "BACKUP_OPTIONS",
    "ENHANCEDINTEGRITY",
];

pub(super) const RESTORE_FLAGS: &[&str] = &[
    "NO_CHECKSUM",
    "CHECKSUM",
    "STOP_ON_ERROR",
    "CONTINUE_AFTER_ERROR",
    "RESTART",
    "REWIND",
    "NOREWIND",
    "UNLOAD",
    "NOUNLOAD",
    "NORECOVERY",
    "RECOVERY",
    "METADATA_ONLY",
    "SNAPSHOT",
    "STATS",
    "REPLACE",
    "RESTRICTED_USER",
    "PARTIAL",
    "KEEP_REPLICATION",
    "KEEP_CDC",
    "ENABLE_BROKER",
    "ERROR_BROKER_CONVERSATIONS",
    "NEW_BROKER",
    "LOADHISTORY",
    "NO_LOG",
    "COMMIT_DIFFERENTIAL_BASE",
    "KEEP_TEMPORAL_RETENTION",
];

pub(super) const RESTORE_VALUES: &[&str] = &[
    "STATS",
    "CREDENTIAL",
    "MEDIANAME",
    "BLOCKSIZE",
    "BUFFERCOUNT",
    "MAXTRANSFERSIZE",
    "STANDBY",
    "PASSWORD",
    "MEDIAPASSWORD",
    "FILE",
    "DBNAME",
    "STOPAT",
    "STOPATMARK",
    "STOPBEFOREMARK",
    "RESTORE_OPTIONS",
    "ENHANCEDINTEGRITY",
];

/// 's' string, 'i' integer (optionally signed), 'v' variable, '?' anything else.
pub(super) fn value_kind(node: Node, source: &[u8]) -> char {
    let text = upper_text(node, source);
    let digits = text.trim_start_matches(['-', '+', ' ']);
    if node.kind() == "literal" && (text.starts_with('\'') || text.starts_with("N'")) {
        's'
    } else if matches!(node.kind(), "literal" | "unary_expression")
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        'i'
    } else if node.kind() == "identifier" && text.starts_with('@') {
        'v'
    } else {
        '?'
    }
}

/// `name = value` with nothing else, and the value's kind; None for any other shape.
pub(super) fn assigned_value(option: Node, source: &[u8]) -> Option<char> {
    let value = option.child_by_field_name("value")?;
    (has_child(option, "=") && option.syntax_named_child_count() == 2)
        .then(|| value_kind(value, source))
}

pub(super) fn backup_file(option: Node, form: &str, source: &[u8], out: &mut Vec<Violation>) {
    let name = option_name(option, source);
    let valid = match name.as_str() {
        "READ_WRITE_FILEGROUPS" => option.syntax_named_child_count() == 1 && form != "keyword_log",
        "FILE" | "FILEGROUP" | "PAGE" => matches!(assigned_value(option, source), Some('s' | 'v')),
        _ => false,
    };
    if !valid {
        out.push(violation(
            "backup-restore-file",
            format!(
                "{name}: a file is FILE, FILEGROUP or PAGE = 'name' or @variable, or READ_WRITE_FILEGROUPS"
            ),
            option,
        ));
    }
}

pub(super) fn backup_device(option: Node, source: &[u8], out: &mut Vec<Violation>) {
    let name = option_name(option, source);
    // A lone name is a logical backup device: a name or a variable.
    let valid = option.syntax_named_child_count() == 1
        || (matches!(
            name.as_str(),
            "DISK" | "TAPE" | "URL" | "DATABASE_SNAPSHOT" | "VIRTUAL_DEVICE"
        ) && matches!(assigned_value(option, source), Some('s' | 'v')));
    if !valid {
        out.push(violation(
            "backup-restore-device",
            format!(
                "{name}: a device is DISK, TAPE, URL, DATABASE_SNAPSHOT or VIRTUAL_DEVICE = 'path' or @variable"
            ),
            option,
        ));
    }
}

pub(super) fn backup_option(option: Node, restore: bool, source: &[u8], out: &mut Vec<Violation>) {
    let name = option_name(option, source);
    let (flags, values) = if restore {
        (RESTORE_FLAGS, RESTORE_VALUES)
    } else {
        (BACKUP_FLAGS, BACKUP_VALUES)
    };
    let known = |list: &[&str]| list.contains(&name.as_str());
    let inner = direct_options(option);
    let value = option.child_by_field_name("value");
    let problem = if has_child(option, "(") {
        match (restore, name.as_str()) {
            (false, "COMPRESSION") => compression_list(&inner, source),
            (false, "ENCRYPTION") => encryption_list(&inner, source),
            (true, "FILESTREAM") => {
                let ok = inner.len() == 1
                    && option_name(inner[0], source) == "DIRECTORY_NAME"
                    && assigned_value(inner[0], source) == Some('s');
                (!ok).then_some("takes (DIRECTORY_NAME = 'name')")
            }
            _ => Some("takes no parenthesized list here"),
        }
    } else if has_child(option, "keyword_to") {
        let target = value.and_then(|v| v.syntax_next_named_sibling()?.syntax_next_named_sibling());
        if !restore {
            Some("belongs to RESTORE")
        } else if [value, target]
            .iter()
            .all(|t| t.is_some_and(|t| matches!(value_kind(t, source), 's' | 'v')))
        {
            None
        } else {
            Some("takes 'logical' TO 'path' (strings or variables)")
        }
    } else if !known(flags) && !known(values) {
        Some("is not an option of this statement")
    } else if value.is_none() {
        (!known(flags)).then_some("needs a value")
    } else if !known(values) {
        Some("takes no value")
    } else {
        let kind = value.map_or('?', |v| value_kind(v, source));
        let after = value
            .and_then(|v| v.syntax_next_named_sibling())
            .filter(|n| n.kind() == "keyword_after")
            .and_then(|n| n.syntax_next_named_sibling());
        if after.is_some() && !matches!(name.as_str(), "STOPATMARK" | "STOPBEFOREMARK") {
            Some("AFTER belongs to STOPATMARK and STOPBEFOREMARK")
        } else if after.is_some_and(|a| !matches!(value_kind(a, source), 's' | 'v')) {
            Some("AFTER takes a string or a variable")
        } else if restore && name == "FILE" && !matches!(kind, 'i' | 'v') {
            Some("takes a backup set number or a variable")
        } else if kind == '?' {
            Some("takes a string, an integer or a variable")
        } else {
            None
        }
    };
    if let Some(problem) = problem {
        out.push(violation(
            "backup-restore-option",
            format!("{name} {problem}"),
            option,
        ));
    }
}

/// COMPRESSION (ALGORITHM = MS_XPRESS | QAT_DEFLATE | ZSTD [, LEVEL = LOW | MEDIUM | HIGH]).
pub(super) fn compression_list(inner: &[Node], source: &[u8]) -> Option<&'static str> {
    let ok = assigned_keyword(
        inner.first(),
        "ALGORITHM",
        &["MS_XPRESS", "QAT_DEFLATE", "ZSTD"],
        source,
    ) && (inner.len() == 1
        || (inner.len() == 2
            && assigned_keyword(inner.get(1), "LEVEL", &["LOW", "MEDIUM", "HIGH"], source)));
    (!ok).then_some(
        "takes (ALGORITHM = MS_XPRESS | QAT_DEFLATE | ZSTD [, LEVEL = LOW | MEDIUM | HIGH])",
    )
}

/// ENCRYPTION (ALGORITHM = AES_128 | AES_192 | AES_256 | TRIPLE_DES_3KEY, SERVER ... = name); the
/// grammar fixes the shape, and the server certificate or key is a name, not a variable.
pub(super) fn encryption_list(inner: &[Node], source: &[u8]) -> Option<&'static str> {
    let algorithm = assigned_keyword(
        inner.first(),
        "ALGORITHM",
        &["AES_128", "AES_192", "AES_256", "TRIPLE_DES_3KEY"],
        source,
    );
    let server = inner
        .get(1)
        .and_then(|o| o.child_by_field_name("value"))
        .is_some_and(|v| value_kind(v, source) != 'v');
    (!(algorithm && server)).then_some(
        "takes (ALGORITHM = AES_128 | AES_192 | AES_256 | TRIPLE_DES_3KEY, SERVER CERTIFICATE | ASYMMETRIC KEY = name)",
    )
}

/// `key = WORD` with WORD one of `words`, written as a bare name.
pub(super) fn assigned_keyword(
    option: Option<&Node>,
    key: &str,
    words: &[&str],
    source: &[u8],
) -> bool {
    option.is_some_and(|o| {
        option_name(*o, source) == key
            && has_child(*o, "=")
            && o.child_by_field_name("value").is_some_and(|v| {
                v.kind() == "identifier" && words.contains(&upper_text(v, source).as_str())
            })
    })
}

/// BACKUP CERTIFICATE ... WITH: one FORMAT = 'string'; the PRIVATE KEY lists together need FILE =
/// 'string' with DER (also the default), and take FILE, ALGORITHM = 'string', ENCRYPTION BY and
/// DECRYPTION BY PASSWORD = 'string' once each. Any non-DER format permits an inline key at parse
/// time (surface-fix-constraints, LocalDB 17.0.1000.7, levels 100-170).
pub(super) fn backup_certificate(children: &[Node], source: &[u8], out: &mut Vec<Violation>) {
    let Some(with) = children.iter().position(|n| n.kind() == "keyword_with") else {
        return;
    };
    let mut in_key = false;
    let mut any_key = false;
    let mut inline_key = false;
    let mut seen = std::collections::BTreeSet::new();
    for node in &children[with..] {
        let (name, ok) = match node.kind() {
            "(" => {
                (in_key, any_key) = (true, true);
                continue;
            }
            ")" => {
                in_key = false;
                continue;
            }
            "option" => {
                let name = option_name(*node, source);
                if !in_key && name == "FORMAT" {
                    inline_key = node.child_by_field_name("value").is_some_and(|v| {
                        let text = upper_text(v, source);
                        let text = text.strip_prefix('N').unwrap_or(&text);
                        string_literal(v, source)
                            && text
                                .strip_prefix('\'')
                                .and_then(|s| s.strip_suffix('\''))
                                .is_some_and(|s| s.trim_end_matches(' ') != "DER")
                    });
                }
                let ok = if in_key {
                    matches!(name.as_str(), "FILE" | "ALGORITHM")
                } else {
                    name == "FORMAT"
                } && assigned_value(*node, source) == Some('s');
                (name, ok)
            }
            "encryption_mechanism" => {
                let name = node
                    .syntax_child(0)
                    .map_or(String::new(), |k| upper_text(k, source));
                let ok = node.syntax_named_child(2).is_some_and(|o| {
                    o.kind() == "option"
                        && option_name(o, source) == "PASSWORD"
                        && assigned_value(o, source) == Some('s')
                });
                (name, ok)
            }
            _ => continue,
        };
        let problem = if !ok {
            format!(
                "{name}: expected FORMAT = 'x' or PRIVATE KEY (FILE | ALGORITHM = 'x', {{ENCRYPTION | DECRYPTION}} BY PASSWORD = 'x')"
            )
        } else if seen.contains(&name) {
            format!("{name} appears more than once")
        } else {
            seen.insert(name);
            continue;
        };
        out.push(violation("backup-certificate-options", problem, *node));
    }
    if any_key && !inline_key && !seen.iter().any(|n| n == "FILE") {
        out.push(violation(
            "backup-certificate-options",
            "DER PRIVATE KEY needs FILE = 'path'".into(),
            children[with],
        ));
    }
}
