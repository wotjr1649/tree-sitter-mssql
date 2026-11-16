use super::Violation;
use super::util::*;
use crate::tree::{has_child, named_children};
use tree_sitter::Node;

const EXTERNAL_LANGUAGE_FILE_OPTIONS: &[&str] = &[
    "CONTENT",
    "FILE_NAME",
    "PLATFORM",
    "PARAMETERS",
    "ENVIRONMENT_VARIABLES",
];
const EXTERNAL_LIBRARY_FILE_OPTIONS: &[&str] = &["CONTENT", "PLATFORM"];
const EXTERNAL_LIBRARY_OPTIONS: &[&str] = &["LANGUAGE"];
const EXTERNAL_POOL_OPTIONS: &[&str] = &[
    "MAX_CPU_PERCENT",
    "MAX_MEMORY_PERCENT",
    "MAX_PROCESSES",
    "AFFINITY",
];
const EXTERNAL_DATA_SOURCE_OPTIONS: &[&str] = &[
    "LOCATION",
    "CREDENTIAL",
    "CONNECTION_OPTIONS",
    "PUSHDOWN",
    "RESOURCE_MANAGER_LOCATION",
    "TYPE",
    "DATABASE_NAME",
    "SHARD_MAP_NAME",
];
const EXTERNAL_MODEL_OPTIONS: &[&str] = &[
    "LOCATION",
    "API_FORMAT",
    "MODEL_TYPE",
    "MODEL",
    "CREDENTIAL",
    "PARAMETERS",
    "LOCAL_RUNTIME_PATH",
];
const EXTERNAL_LANGUAGE_REQUIRED: &[&str] = &["CONTENT", "FILE_NAME"];
const EXTERNAL_LIBRARY_REQUIRED: &[&str] = &["CONTENT"];
const ENDPOINT_TCP_OPTIONS: &[&str] = &["LISTENER_PORT", "LISTENER_IP"];
const ENDPOINT_TSQL_OPTIONS: &[&str] = &["ENCRYPTION"];
const ENDPOINT_BROKER_OPTIONS: &[&str] = &[
    "AUTHENTICATION",
    "ENCRYPTION",
    "MESSAGE_FORWARDING",
    "MESSAGE_FORWARD_SIZE",
];
const ENDPOINT_MIRROR_OPTIONS: &[&str] = &["AUTHENTICATION", "ENCRYPTION", "ROLE"];

/// EXTERNAL option sets measured with LocalDB 17.0.1000.7 at levels 100-170:
/// ddl-external/external-* duplicates are Msg 102/1039; CREATE LANGUAGE requires CONTENT and
/// FILE_NAME (39123), CREATE LIBRARY requires CONTENT (319); both ALTER forms may omit CONTENT.
/// Missing model LOCATION, file-format FORMAT_TYPE and external-table DATA_SOURCE are accepted
/// by PARSEONLY, so they are deliberately not required here (ADR-0003).
pub(super) fn external_options(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let kind = statement.kind();
    let language = kind.ends_with("_language");
    let library = kind.ends_with("_library");
    if language {
        let children = named_children(statement);
        if has_child(statement, "keyword_remove")
            && let Some(platform) = children.last()
            && !matches!(upper_text(*platform, source).as_str(), "WINDOWS" | "LINUX")
        {
            out.push(violation(
                "external-platform",
                "PLATFORM must be WINDOWS or LINUX".into(),
                *platform,
            ));
        }
    }
    let containers: Vec<Node> = named_children(statement)
        .into_iter()
        .filter(|n| matches!(n.kind(), "external_file_spec" | "with_options"))
        .collect();
    // ALTER DATA SOURCE/MODEL has its option list directly on the statement.
    let containers = if containers.is_empty()
        && matches!(kind, "alter_external_data_source" | "alter_external_model")
    {
        vec![statement]
    } else {
        containers
    };
    for container in containers {
        let file = container.kind() == "external_file_spec";
        let allowed: &[&str] = if file && language {
            EXTERNAL_LANGUAGE_FILE_OPTIONS
        } else if file && library {
            EXTERNAL_LIBRARY_FILE_OPTIONS
        } else if library {
            EXTERNAL_LIBRARY_OPTIONS
        } else if kind.ends_with("_resource_pool") {
            EXTERNAL_POOL_OPTIONS
        } else if kind.ends_with("_data_source") {
            EXTERNAL_DATA_SOURCE_OPTIONS
        } else {
            EXTERNAL_MODEL_OPTIONS
        };
        let mut seen = std::collections::BTreeSet::new();
        for option in direct_options(container) {
            let Some(name) = option.child_by_field_name("name") else {
                continue;
            };
            let key = option_name(option, source);
            if !seen.insert(key.clone()) {
                out.push(violation(
                    "external-option-duplicate",
                    format!("{key} appears more than once"),
                    option,
                ));
            }
            if !allowed.contains(&key.as_str()) {
                out.push(violation(
                    "external-option",
                    format!("{key} is not valid in this EXTERNAL option set"),
                    option,
                ));
            }
            if key == "AFFINITY" {
                let children = named_children(option);
                if let Some(scalar) = children
                    .iter()
                    .find(|n| n.kind() == "identifier" && n.id() != name.id())
                    && (upper_text(*scalar, source) != "AUTO" || !has_child(option, "keyword_cpu"))
                {
                    out.push(violation(
                        "external-affinity",
                        "only AFFINITY CPU accepts AUTO; otherwise use a range list".into(),
                        *scalar,
                    ));
                }
            }
            if !has_child(option, "=") {
                out.push(violation(
                    "external-option-assignment",
                    format!("{key} requires '='"),
                    option,
                ));
            }
            if key == "PLATFORM"
                && let Some(value) = option.child_by_field_name("value")
                && !matches!(upper_text(value, source).as_str(), "WINDOWS" | "LINUX")
            {
                out.push(violation(
                    "external-platform",
                    "PLATFORM must be WINDOWS or LINUX".into(),
                    value,
                ));
            }
        }
        let required: &[&str] = if file && kind == "create_external_language" {
            EXTERNAL_LANGUAGE_REQUIRED
        } else if file && kind == "create_external_library" {
            EXTERNAL_LIBRARY_REQUIRED
        } else if library && !file {
            EXTERNAL_LIBRARY_OPTIONS
        } else {
            &[]
        };
        for key in required {
            if !seen.contains(*key) {
                out.push(violation(
                    "external-option-required",
                    format!("{kind} requires {key}"),
                    container,
                ));
            }
        }
    }
}

/// FULLTEXT options appear at most once; CHANGE_TRACKING names a mode and NO POPULATION needs
/// CHANGE_TRACKING OFF in CREATE. Evidence: LocalDB 17.0.1000.7, levels 100-170,
/// ddl-external/fulltext-{catalog-accent,index-change,index-stoplist,index-search}-duplicate,
/// index-change-unknown and index-no-population-alone: Msg 102; index-no-population-auto: Msg
/// 7663. CREATE contains both settings, so their conflict needs no catalog state (ADR-0003).
pub(super) fn fulltext_options(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let mut seen = std::collections::BTreeSet::new();
    let mut tracking_off = false;
    let mut no_population = None;
    for clause in named_children(statement)
        .into_iter()
        .filter(|n| matches!(n.kind(), "with_clause" | "with_options"))
    {
        for option in direct_options(clause) {
            if option.child_by_field_name("name").is_none() {
                continue;
            }
            let key = option_name(option, source);
            if !seen.insert(key.clone()) {
                out.push(violation(
                    "fulltext-option-duplicate",
                    format!("{key} appears more than once in FULLTEXT options"),
                    option,
                ));
            }
            if statement.kind() != "create_fulltext_index" {
                continue;
            }
            let value = option
                .child_by_field_name("value")
                .map(|n| upper_text(n, source))
                .unwrap_or_default();
            if key == "CHANGE_TRACKING" {
                tracking_off |= value == "OFF";
                if !matches!(value.as_str(), "AUTO" | "MANUAL" | "OFF") {
                    out.push(violation(
                        "fulltext-change-tracking",
                        "CHANGE_TRACKING needs AUTO, MANUAL or OFF".into(),
                        option,
                    ));
                }
            } else if key == "NO" && value == "POPULATION" {
                no_population = Some(option);
            }
        }
    }
    if let Some(option) = no_population
        && !tracking_off
    {
        out.push(violation(
            "fulltext-no-population",
            "CREATE FULLTEXT INDEX NO POPULATION needs CHANGE_TRACKING OFF".into(),
            option,
        ));
    }
}

/// Endpoint option sets and enum values checked by the engine while parsing (ADR-0003).
/// Evidence: LocalDB 17.0.1000.7, levels 100-170, ddl-external/endpoint-* probes:
/// duplicate options Msg 7816, CREATE's missing LISTENER_PORT/ROLE Msg 7801, unknown options,
/// protocol/algorithm/role Msg 102, MESSAGE_FORWARDING = ON Msg 156, IPv4 octets Msg 7810,
/// CREATE STATE = DISABLED Msg 7871. TSQL ENCRYPTION = REQUIRED
/// and SERVICE_BROKER ENCRYPTION = STRICT are accepted, so no payload-specific mode check.
pub(super) fn endpoint_options(statement: Node, source: &[u8], out: &mut Vec<Violation>) {
    let creating = statement.kind() == "create_endpoint";
    for clause in named_children(statement) {
        let kind = clause.kind();
        if creating
            && kind == "endpoint_state"
            && clause
                .child_by_field_name("value")
                .map(|n| upper_text(n, source))
                == Some("DISABLED".into())
        {
            out.push(violation(
                "endpoint-create-state",
                "CREATE ENDPOINT cannot specify STATE = DISABLED".into(),
                clause,
            ));
        }
        if !matches!(kind, "endpoint_transport" | "endpoint_payload") {
            continue;
        }
        let payload = clause
            .child_by_field_name("type")
            .map(|n| upper_text(n, source))
            .unwrap_or_default();
        let allowed: &[&str] = match (kind, payload.as_str()) {
            ("endpoint_transport", _) => ENDPOINT_TCP_OPTIONS,
            (_, "TSQL") => ENDPOINT_TSQL_OPTIONS,
            (_, "SERVICE_BROKER") => ENDPOINT_BROKER_OPTIONS,
            (_, "DATABASE_MIRRORING") => ENDPOINT_MIRROR_OPTIONS,
            _ => &[],
        };
        let mut seen = std::collections::BTreeSet::new();
        for option in direct_options(clause) {
            if option.child_by_field_name("name").is_none() {
                continue;
            }
            let name = option_name(option, source);
            if !allowed.contains(&name.as_str()) {
                out.push(violation(
                    "endpoint-option",
                    format!("{name} is not valid in this endpoint option set"),
                    option,
                ));
                continue;
            }
            if !seen.insert(name.clone()) {
                out.push(violation(
                    "endpoint-duplicate",
                    format!("{name} occurs more than once"),
                    option,
                ));
            }
            if let Some(protocol) = option.child_by_field_name("protocol")
                && !matches!(
                    upper_text(protocol, source).as_str(),
                    "NTLM" | "KERBEROS" | "NEGOTIATE"
                )
            {
                out.push(violation(
                    "endpoint-authentication",
                    "WINDOWS protocol must be NTLM, KERBEROS or NEGOTIATE".into(),
                    protocol,
                ));
            }
            if let Some(value) = option.child_by_field_name("value") {
                let text = upper_text(value, source);
                let valid = match name.as_str() {
                    "ROLE" => matches!(text.as_str(), "WITNESS" | "PARTNER" | "ALL"),
                    "ENCRYPTION" => matches!(
                        text.as_str(),
                        "DISABLED" | "SUPPORTED" | "REQUIRED" | "NEGOTIATED" | "STRICT"
                    ),
                    "MESSAGE_FORWARDING" => matches!(text.as_str(), "ENABLED" | "DISABLED"),
                    "LISTENER_IP"
                        if !text.starts_with("'") && !text.starts_with("N'") && text != "ALL" =>
                    {
                        text.split('.').count() == 4
                            && text.split('.').all(|part| part.parse::<u8>().is_ok())
                    }
                    _ => true,
                };
                if !valid {
                    out.push(violation(
                        "endpoint-value",
                        format!("invalid {name} value"),
                        value,
                    ));
                }
            }
            let mut algorithm_cursor = option.walk();
            let algorithms: Vec<Node> = option
                .children_by_field_name("algorithm", &mut algorithm_cursor)
                .collect();
            let mut seen_algorithms = std::collections::BTreeSet::new();
            for algorithm in &algorithms {
                let text = upper_text(*algorithm, source);
                if !matches!(text.as_str(), "AES" | "RC4") || !seen_algorithms.insert(text) {
                    out.push(violation(
                        "endpoint-algorithm",
                        "ALGORITHM accepts AES, RC4 or each once in either order".into(),
                        *algorithm,
                    ));
                }
            }
            if !algorithms.is_empty()
                && option
                    .child_by_field_name("value")
                    .map(|n| upper_text(n, source))
                    == Some("DISABLED".into())
            {
                out.push(violation(
                    "endpoint-algorithm",
                    "DISABLED cannot specify ALGORITHM".into(),
                    option,
                ));
            }
        }
        let required = if !creating {
            None
        } else if kind == "endpoint_transport" {
            Some("LISTENER_PORT")
        } else if payload == "DATABASE_MIRRORING" {
            Some("ROLE")
        } else {
            None
        };
        if let Some(name) = required
            && !seen.contains(name)
        {
            out.push(violation(
                "endpoint-required",
                format!("CREATE ENDPOINT requires {name}"),
                clause,
            ));
        }
    }
}
