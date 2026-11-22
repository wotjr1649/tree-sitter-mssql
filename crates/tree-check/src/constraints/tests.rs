use super::util::SyntaxNode;
use super::*;
use tree_sitter::Parser;

fn parse(sql: &str) -> Tree {
    let mut p = Parser::new();
    p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
    p.parse(sql, None).unwrap()
}

fn rules(sql: &str) -> Vec<&'static str> {
    let mut p = Parser::new();
    p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
    let tree = p.parse(sql, None).unwrap();
    check(&tree, sql.as_bytes())
        .into_iter()
        .map(|v| v.rule)
        .collect()
}

fn engine_accepts(verdict: &serde_json::Value, id: &str) -> bool {
    verdict["verdicts"][id]
        .as_object()
        .unwrap()
        .values()
        .any(|v| v.as_array().is_some_and(|a| a.is_empty()))
}

fn probe_cases(probes: &str) -> impl Iterator<Item = serde_json::Value> + '_ {
    probes
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
}

#[test]
fn issue_87_reports_the_reference_name_and_each_stray_column_word() {
    let case = probe_cases(include_str!(
        "../../../../oracle/data/probes/batch-rules.jsonl"
    ))
    .find(|case| case["id"] == "batch-rules/issue-87-step-5")
    .unwrap();
    let sql = case["source"].as_str().unwrap();
    let tree = parse(sql);
    assert!(!tree.root_node().has_error());
    let violations = check(&tree, sql.as_bytes());
    let line = sql.lines().nth(1).unwrap();
    let words: Vec<_> = violations
        .iter()
        .map(|v| {
            assert_eq!(v.row, 1);
            (v.rule, line[v.column..].split_whitespace().next().unwrap())
        })
        .collect();
    assert_eq!(
        words,
        [
            ("gap-reserved-name", "ON"),
            ("column-unknown-option", "DELETE"),
            ("column-unknown-option", "CASCADE"),
            ("column-unknown-option", "ON"),
            ("column-unknown-option", "UPDATE"),
            ("column-unknown-option", "SET"),
        ]
    );
}

#[test]
fn column_word_checks_preserve_the_flattened_index_placement() {
    for (sql, expected) in [
        (
            "CREATE TABLE t (id int INDEX ix NONCLUSTERED FILESTREAM_ON /* fg */ [default]);",
            &[][..],
        ),
        (
            "CREATE TABLE t (id int INDEX ix FILESTREAM_ON fg BoGuS);",
            &["column-unknown-option"][..],
        ),
        (
            "CREATE TABLE t (id int [SPARSE]);",
            &["column-unknown-option"][..],
        ),
        (
            "CREATE TABLE t (id int CONSTRAINT RoWgUiDcOl UNIQUE);",
            &["gap-reserved-name"][..],
        ),
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert_eq!(rules(sql), expected, "{sql}");
    }
}

#[test]
fn issue_87_review_forms_pass_in_both_sessions() {
    let cases: Vec<_> = probe_cases(include_str!(
        "../../../../oracle/data/probes/batch-rules.jsonl"
    ))
    .filter(|case| {
        case["id"]
            .as_str()
            .unwrap()
            .starts_with("batch-rules/review-")
    })
    .collect();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let sql = case["source"].as_str().unwrap();
        let tree = parse(sql);
        for setting in [crate::QuotedIdentifier::On, crate::QuotedIdentifier::Off] {
            let report = crate::inspect_with_quoted_identifier(&tree, sql.as_bytes(), setting);
            assert!(report.ok(), "{sql} ({setting:?}): {report:?}");
        }
    }
}

#[test]
fn issue_87_review_exceptions_are_limited_to_the_syntax_words() {
    for (sql, rule) in [
        ("SELECT f(DOUBLE PRECISION, 1);", "gap-reserved-name"),
        (
            "SELECT CONVERT(DOUBLE AS PRECISION, 1);",
            "gap-reserved-name",
        ),
        ("SELECT CONVERT(int, char varying);", "gap-reserved-name"),
        ("SELECT CONVERT(dbo.foo varying, 1);", "gap-reserved-name"),
        ("SELECT DATABASE();", "gap-reserved-name"),
        ("SELECT {fn CONCAT(DATABASE(), 'x')};", "gap-reserved-name"),
        ("CREATE TABLE t (TIMESTAMP NOT);", "gap-reserved-name"),
        (
            "CREATE TABLE t (TIMESTAMP NOT(1) NULL);",
            "gap-reserved-name",
        ),
        ("CREATE TABLE t (c int varying);", "column-unknown-option"),
        (
            "CREATE TABLE t (c char(10) varying);",
            "column-unknown-option",
        ),
        (
            "CREATE TABLE t (c char varying varying);",
            "column-unknown-option",
        ),
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert_eq!(rules(sql), [rule], "{sql}");
    }
}

#[test]
fn comments_do_not_change_constraint_rules() {
    for sql in [
        "SELECT 1 FROM t WHERE a = 1 AND (\n b = 0 OR c = 1);",
        "SELECT 1 FROM t WHERE ( b = 0);",
        "IF (\n 1 = 1) PRINT 1;",
        "SELECT 1\nWITH c AS (SELECT 1 AS a) SELECT * FROM c;",
        "GRANT SELECT (a) ON dbo.t TO u;",
        "GRANT DELETE (a) ON dbo.t TO u;",
        "BACKUP DATABASE d TO DISK = 'a';",
        "BACKUP DATABASE d TO PIPE = 'a';",
        "BACKUP DATABASE d TO DISK = 'a' WITH STATS = + 5;",
        "BACKUP CERTIFICATE c TO FILE = 'a' WITH FORMAT = 'PFX', FORMAT = 'PFX';",
        "SELECT * FROM OPENJSON('{}') WITH (a int);",
        "EXEC ('p', @v OUTPUT) AT s;",
        "SELECT * FROM dbo.f() WITH (NOLOCK);",
    ] {
        let expected = rules(sql);
        for commented in [
            sql.replace(' ', " /* c */ ").replace('\n', "\n-- c\n"),
            sql.replace(' ', " /* outer /* inner */ c */ "),
        ] {
            assert!(!parse(&commented).root_node().has_error(), "{commented}");
            assert_eq!(rules(&commented), expected, "{commented}");
        }
    }
}

#[test]
fn comments_do_not_change_error_recovery_constraints() {
    for sql in [
        "BACKUP DATABASE 'd' TO DISK = 'a.bak';",
        "BACKUP DATABASE d TO 'a';",
        "RESTORE DATABASE 'd' FROM DISK = 'a.bak';",
        "BACKUP DATABASE d TO DISK = 'a' WITH STATS = 5 + 5;",
        "SELECT TOP (10) WITH APPROXIMATE t.c1 FROM VECTOR_SEARCH(TABLE = dbo.docs AS t, COLUMN = v1, SIMILAR_TO = @q) AS r;",
    ] {
        assert!(parse(sql).root_node().has_error(), "{sql}");
        let commented = sql.replace(' ', " /* 기밀 C:\\audit */ ");
        assert!(parse(&commented).root_node().has_error(), "{commented}");
        for variant in [
            sql.to_string(),
            commented,
            sql.replace(' ', "\n"),
            sql.replace(' ', "\t\r\n"),
        ] {
            assert!(parse(&variant).root_node().has_error(), "{variant}");
            assert!(rules(&variant).is_empty(), "{variant}");
        }
    }
}

#[test]
fn long_search_conditions_do_not_overflow_the_stack() {
    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(|| {
            let predicates = vec!["1=1"; 16_384].join(" AND ");
            let sql = format!("SELECT 1 WHERE {predicates};");
            assert!(rules(&sql).is_empty());
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn surface_constraint_neighbours_match_the_engine() {
    let verdict: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/data/verdicts/localdb-17.0.1000.7.surface-fix-constraints.json"
    ))
    .unwrap();
    for case in probe_cases(include_str!(
        "../../../../oracle/data/probes/surface-fix-constraints.jsonl"
    )) {
        let id = case["id"].as_str().unwrap();
        let sql = case["source"].as_str().unwrap();
        let accepted = engine_accepts(&verdict, id);
        assert_eq!(rules(sql).is_empty(), accepted, "{id}: {:?}", rules(sql));
    }
}

#[test]
fn surface_gap_neighbours_match_the_engine() {
    let verdict: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/data/verdicts/localdb-17.0.1000.7.surface-fix-gaps.json"
    ))
    .unwrap();
    for case in probe_cases(include_str!(
        "../../../../oracle/data/probes/surface-fix-gaps.jsonl"
    )) {
        let id = case["id"].as_str().unwrap();
        let sql = case["source"].as_str().unwrap();
        let accepted = engine_accepts(&verdict, id);
        let tree = parse(sql);
        assert_eq!(
            !tree.root_node().has_error() && rules(sql).is_empty(),
            accepted,
            "{id}: {:?}",
            rules(sql)
        );
    }
}

#[test]
fn surface_ddl_classification_neighbours_match_the_engine() {
    let verdict: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/data/verdicts/localdb-17.0.1000.7.surface-fix-ddl-classification.json"
    ))
    .unwrap();
    for case in probe_cases(include_str!(
        "../../../../oracle/data/probes/surface-fix-ddl-classification.jsonl"
    )) {
        let id = case["id"].as_str().unwrap();
        let sql = case["source"].as_str().unwrap();
        let accepted = engine_accepts(&verdict, id);
        let tree = parse(sql);
        if accepted {
            assert!(
                !tree.root_node().has_error() && rules(sql).is_empty(),
                "{id}: {:?}",
                rules(sql)
            );
        } else if id.ends_with("crypto_with_enable")
            || id.ends_with("crypto_with_disable")
            || id.ends_with("crypto_with_file")
            || id.ends_with("grant_language_user")
            || id.ends_with("grant_library_user")
            || id.ends_with("cetas_format_options")
            || id.ends_with("cetas_format_options_file")
            || id.ends_with("cetas_bad_option")
            || id.ends_with("ctas_declared_variable")
            || id.ends_with("ctas_undeclared_variable")
            || id.ends_with("ctas_variable_where")
            || id.ends_with("ctas_variable_function")
            || id.ends_with("ctas_subquery_variable")
            || id.ends_with("ctas_cte_variable")
        {
            assert!(!rules(sql).is_empty(), "{id}");
        } else {
            assert!(
                rules(sql).is_empty(),
                "platform/context rejection {id}: {:?}",
                rules(sql)
            );
        }
    }
}

#[test]
fn cetas_options_preserve_documented_platform_shapes() {
    let verdict: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/data/verdicts/localdb-17.0.1000.7.surface-fix-cetas-options.json"
    ))
    .unwrap();
    for case in probe_cases(include_str!(
        "../../../../oracle/data/probes/surface-fix-cetas-options.jsonl"
    )) {
        let id = case["id"].as_str().unwrap();
        let sql = case["source"].as_str().unwrap();
        let invalid = id.ends_with("/inline-format-options") || id.ends_with("/unknown-option");
        assert!(!parse(sql).root_node().has_error(), "{id}");
        assert_eq!(rules(sql).is_empty(), !invalid, "{id}: {:?}", rules(sql));
        if invalid {
            assert!(
                verdict["verdicts"][id]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(|v| v.as_array().is_some_and(|a| a == &[serde_json::json!(102)])),
                "{id}"
            );
        } else {
            assert!(
                verdict["verdicts"][id]
                    .as_object()
                    .unwrap()
                    .values()
                    .all(|v| v.as_array().is_some_and(|a| a.is_empty()
                        || a == &[serde_json::json!(46553)]
                        || id.ends_with("/mi-partition"))),
                "{id}"
            );
        }
    }
}

#[test]
fn gap_forms_match_engine_evidence() {
    let verdict: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/data/verdicts/localdb-17.0.1000.7.gap-forms.json"
    ))
    .unwrap();
    for case in probe_cases(include_str!(
        "../../../../oracle/data/probes/gap-forms.jsonl"
    )) {
        let id = case["id"].as_str().unwrap();
        let sql = case["source"].as_str().unwrap();
        let positive = engine_accepts(&verdict, id);
        let clean = !parse(sql).root_node().has_error() && rules(sql).is_empty();
        // These rejections need declarations, preview/edition settings or catalog state.
        let external = matches!(
            id,
            "gap-forms/original/ddl-rest/commit-off"
                | "gap-forms/original/kit-routes/alt-tsql-V22-r018"
                | "gap-forms/vector/aliased"
                | "gap-forms/vector/no-alias"
                | "gap-forms/property/scalar"
                | "gap-forms/external/audit-operator"
                | "gap-forms/rowset/bulk-ordinal"
                | "gap-forms/commit/variant-341"
                | "gap-forms/commit/variant-343"
        );
        assert_eq!(clean, positive || external, "{id}: {sql}");
    }
}

#[test]
fn last_forms_match_engine_evidence() {
    let verdict: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/data/verdicts/localdb-17.0.1000.7.last-forms.json"
    ))
    .unwrap();
    for case in probe_cases(include_str!(
        "../../../../oracle/data/probes/last-forms.jsonl"
    )) {
        let id = case["id"].as_str().unwrap();
        let sql = case["source"].as_str().unwrap();
        let positive = engine_accepts(&verdict, id);
        let clean = !parse(sql).root_node().has_error() && rules(sql).is_empty();
        assert_eq!(clean, positive, "{id}: {sql}");
    }
    for (sql, expected) in [
        (
            "SELECT 1 WHERE x = 'a' ESCAPE '!';",
            "expression-like-escape",
        ),
        (
            "SELECT JSON_OBJECTAGG('k':x ORDER BY y) FROM t;",
            "expression-json-aggregate",
        ),
        (
            "SELECT * FROM (SELECT 1 UNION SELECT 2 INTO #t) d;",
            "query-select-into-scope",
        ),
    ] {
        assert_eq!(rules(sql), [expected], "{sql}");
    }
}

#[test]
fn ddl_rest_routes_match_engine_boundaries() {
    assert_eq!(
        rules("SELECT 1 FROM a,e,f,b WHERE MATCH(a-(e|f)->b);"),
        ["ddl-rest-graph-edge"]
    );
    for (sql, rule) in [
        (
            "SELECT ALL DISTINCT c FROM dbo.t;",
            "query-select-quantifier",
        ),
        (
            "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){1,0}));",
            "ddl-rest-graph-bound",
        ),
        (
            "SELECT SUM(v) IGNORE NULLS OVER(ORDER BY id) FROM t;",
            "ddl-rest-nulls-modifier",
        ),
        (
            "SELECT ROW_NUMBER() RESPECT NULLS OVER(ORDER BY id) FROM t;",
            "ddl-rest-nulls-modifier",
        ),
        (
            "SELECT 1 FROM a,e,b WHERE MATCH(a-(e)->b,b<-(e)-a);",
            "ddl-rest-graph-match",
        ),
        (
            "SELECT 1 FROM a,e,b WHERE MATCH(SHORTEST_PATH(a-(e)->b+));",
            "ddl-rest-graph-shortest",
        ),
        (
            "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){0,3}));",
            "ddl-rest-graph-bound",
        ),
        (
            "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){2,3}));",
            "ddl-rest-graph-bound",
        ),
    ] {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_mssql::LANGUAGE.into())
            .unwrap();
        assert!(
            !parser.parse(sql, None).unwrap().root_node().has_error(),
            "{sql}: checked by constraints"
        );
        assert_eq!(rules(sql), [rule], "{sql}");
    }
    for sql in [
        "SELECT FIRST_VALUE(v) IGNORE NULLS OVER(ORDER BY id) FROM t;",
        "SELECT LAST_VALUE(v) RESPECT NULLS OVER(ORDER BY id) FROM t;",
        "SELECT LAG(v) IGNORE NULLS OVER(ORDER BY id) FROM t;",
        "SELECT LEAD(v) RESPECT NULLS OVER(ORDER BY id) FROM t;",
        "SELECT 1 FROM a,e,b WHERE MATCH(a-(e)->b AND b<-(e)-a);",
        "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH(a(-(e)->b){1,3}));",
        "SELECT 1 FROM a,e FOR PATH e,b FOR PATH b WHERE MATCH(SHORTEST_PATH((a<-(e)-)+b));",
        "dbo.p; SELECT 1;",
        "SELECT 1;\nGO\ndbo.p;",
        "/* 한글 C:\\work */ dbo.p 1;",
    ] {
        assert!(rules(sql).is_empty(), "{sql}");
    }
    for sql in ["SELECT 1; dbo.p;", ";dbo.p;", "start: dbo.p;"] {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_mssql::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(sql, None).unwrap();
        assert!(
            tree.root_node().has_error() || rules(sql) == ["ddl-rest-implicit-exec"],
            "{sql}"
        );
    }
}

#[test]
fn rest_create_constraints_follow_engine_neighbours() {
    for (sql, expected) in [
        (
            "CREATE AGGREGATE dbo.a () RETURNS int EXTERNAL NAME asm.[Ns.Class];",
            "ddl-rest-aggregate",
        ),
        (
            "CREATE AGGREGATE dbo.a (@v int = 1) RETURNS int EXTERNAL NAME asm.[Ns.Class];",
            "ddl-rest-aggregate",
        ),
        (
            "CREATE CRYPTOGRAPHIC PROVIDER p FROM FILE = 0x1234;",
            "ddl-rest-provider-file",
        ),
        (
            "CREATE ASYMMETRIC KEY k FROM EXECUTABLE = 'C:\\a.dll';",
            "ddl-rest-key-source",
        ),
        (
            "CREATE EVENT SESSION e ON SERVER;",
            "ddl-rest-event-session",
        ),
        (
            "CREATE INDEX i ON dbo.t(c) USING GEOMETRY_GRID;",
            "ddl-rest-index-form",
        ),
        (
            "CREATE JSON INDEX i ON dbo.t(j) FOR (p = '$.a');",
            "ddl-rest-index-form",
        ),
        ("CREATE JSON INDEX i ON dbo.t(a,b);", "ddl-rest-index-form"),
        (
            "CREATE SPATIAL INDEX i ON dbo.t(a,b);",
            "ddl-rest-index-form",
        ),
        (
            "CREATE XML INDEX i ON dbo.t(x) USING XML INDEX p FOR WRONG;",
            "ddl-rest-index-form",
        ),
        (
            "CREATE SELECTIVE XML INDEX i ON dbo.t(x);",
            "ddl-rest-index-form",
        ),
        (
            "CREATE PRIMARY XML INDEX i ON dbo.t(x) USING XML INDEX p;",
            "ddl-rest-index-form",
        ),
        (
            "CREATE INDEX i ON dbo.t(c) FOR ('$.a');",
            "ddl-rest-index-form",
        ),
        (
            "CREATE UNIQUE JSON INDEX i ON dbo.t(j);",
            "ddl-rest-index-form",
        ),
    ] {
        assert!(
            parse(sql).root_node().has_error() || rules(sql).contains(&expected),
            "{sql}: {expected}"
        );
    }
    for sql in [
        "CREATE AGGREGATE dbo.a (@v int) RETURNS int EXTERNAL NAME asm.[Ns.Class];",
        "CREATE CRYPTOGRAPHIC PROVIDER p FROM FILE = N'C:\\provider.dll';",
        "SET QUOTED_IDENTIFIER OFF; CREATE CRYPTOGRAPHIC PROVIDER p FROM FILE = \"provider.dll\";",
        "CREATE ASYMMETRIC KEY k FROM EXECUTABLE FILE = 'C:\\a.dll';",
        "CREATE EVENT SESSION e ON DATABASE ADD EVENT sqlserver.error_reported WITH (MAX_DURATION = 2 MINUTES);",
        "CREATE JSON INDEX i ON dbo.t(j) FOR (N'$.a', N'$.b');",
        "CREATE SELECTIVE XML INDEX i ON dbo.t(x) FOR (p = '/a' AS XQUERY 'xs:string');",
        "CREATE PRIMARY XML INDEX i ON dbo.t(x);",
        "CREATE XML INDEX i ON dbo.t(x) USING XML INDEX p FOR PATH;",
        "CREATE XML INDEX i ON dbo.t(x) USING XML INDEX p FOR (a);",
        "CREATE SPATIAL INDEX i ON dbo.t(g) USING BOGUS;",
        "CREATE SPATIAL INDEX i ON dbo.t(g) USING GEOMETRY_GRID WITH (BOUNDING_BOX = (0,0,10));",
        "CREATE APPLICATION ROLE a WITH DEFAULT_SCHEMA = dbo;",
    ] {
        assert!(rules(sql).is_empty(), "{sql}: {:?}", rules(sql));
    }
}

#[test]
fn ddl_rest_alter_neighbours_match_the_engine() {
    for (sql, rule) in [
        (
            "ALTER CREDENTIAL c WITH SECRET = 'x';",
            "ddl-rest-credential",
        ),
        (
            "ALTER DATABASE SCOPED CREDENTIAL c WITH SECRET = 'x';",
            "ddl-rest-credential",
        ),
        (
            "CREATE CREDENTIAL c WITH SECRET = 'placeholder';",
            "ddl-rest-credential",
        ),
        (
            "CREATE REMOTE SERVICE BINDING b TO SERVICE 's' WITH ANONYMOUS = OFF;",
            "ddl-rest-remote-binding",
        ),
        (
            "CREATE RESOURCE POOL p WITH (AFFINITY NUMANODE = AUTO);",
            "ddl-rest-resource-pool",
        ),
        (
            "CREATE RESOURCE POOL p WITH (MAX_CPU_PERCENT = 101);",
            "ddl-rest-resource-pool",
        ),
        (
            "CREATE WORKLOAD GROUP g WITH (IMPORTANCE = WRONG);",
            "ddl-rest-workload-group",
        ),
        (
            "CREATE LOGIN l WITH PASSWORD = 'dummy' OLD_PASSWORD = 'other';",
            "ddl-rest-principal",
        ),
        (
            "ALTER QUEUE q REORGANIZE WITH LOB_COMPACTION = OFF;",
            "ddl-rest-queue",
        ),
        (
            "ALTER MESSAGE TYPE m VALIDATION = INVALID;",
            "ddl-rest-message-type",
        ),
        (
            "ALTER MESSAGE TYPE m VALIDATION = NONE WITH SCHEMA COLLECTION dbo.x;",
            "ddl-rest-message-type",
        ),
        (
            "ALTER AUTHORIZATION ON ASYMMETRIC::k TO dbo;",
            "ddl-rest-authorization",
        ),
    ] {
        assert!(
            parse(sql).root_node().has_error() || rules(sql).contains(&rule),
            "{sql}: {rule}"
        );
    }
    for sql in [
        "ALTER REMOTE SERVICE BINDING b WITH ANONYMOUS = OFF;",
        "CREATE APPLICATION ROLE a WITH DEFAULT_SCHEMA = dbo;",
        "CREATE CREDENTIAL c WITH IDENTITY = 'dummy';",
        "ALTER CREDENTIAL c WITH IDENTITY = 'dummy', SECRET = 'placeholder';",
        "CREATE REMOTE SERVICE BINDING b TO SERVICE 's' WITH USER = u, ANONYMOUS = OFF;",
        "CREATE RESOURCE POOL p WITH (AFFINITY SCHEDULER = AUTO, MAX_CPU_PERCENT = 50);",
        "ALTER RESOURCE POOL p WITH (AFFINITY NUMANODE = (0, 1 TO 2));",
        "CREATE WORKLOAD GROUP g WITH (IMPORTANCE = MEDIUM) USING p;",
        "ALTER WORKLOAD GROUP g WITH (IMPORTANCE = HIGH);",
        "CREATE BROKER PRIORITY p FOR CONVERSATION SET (PRIORITY_LEVEL = 11);",
        "ALTER LOGIN l WITH PASSWORD = 'dummy' OLD_PASSWORD = 'other';",
        "CREATE LOGIN l WITH PASSWORD = 'dummy' MUST_CHANGE, CHECK_POLICY = ON;",
        "ALTER QUEUE q REORGANIZE WITH (LOB_COMPACTION = OFF);",
        "ALTER MESSAGE TYPE m VALIDATION = VALID_XML WITH SCHEMA COLLECTION dbo.x;",
        "ALTER AUTHORIZATION ON ASYMMETRIC KEY::k TO dbo;",
    ] {
        assert!(rules(sql).is_empty(), "{sql}: {:?}", rules(sql));
    }
}

#[test]
fn ddl_rest_permission_tails_match_the_engine() {
    for sql in [
        "GRANT SELECT ON dbo.t FROM u;",
        "DENY SELECT ON dbo.t FROM u;",
        "GRANT SELECT ON dbo.t TO u CASCADE;",
        "DENY SELECT ON dbo.t TO u WITH GRANT OPTION;",
        "REVOKE SELECT ON dbo.t FROM u WITH GRANT OPTION;",
        "GRANT GRANT OPTION FOR SELECT ON dbo.t TO u;",
        "DENY GRANT OPTION FOR SELECT ON dbo.t TO u;",
    ] {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_mssql::LANGUAGE.into())
            .unwrap();
        assert!(
            !parser.parse(sql, None).unwrap().root_node().has_error(),
            "{sql}: this boundary is enforced by constraints"
        );
        assert_eq!(rules(sql), ["ddl-rest-permission"], "{sql}");
    }
    for sql in [
        "GRANT SELECT ON dbo.t TO u WITH GRANT OPTION AS dbo;",
        "DENY SELECT ON dbo.t TO u CASCADE AS dbo;",
        "REVOKE SELECT ON dbo.t TO u;",
        "REVOKE SELECT ON dbo.t FROM u CASCADE AS dbo;",
        "REVOKE GRANT OPTION FOR SELECT ON dbo.t TO u CASCADE AS dbo;",
        r"GRANT CONTROL ON ASYMMETRIC/* 한국어 C:\keys */KEY::ak TO u;",
    ] {
        assert!(rules(sql).is_empty(), "{sql}");
    }
}

#[test]
fn expression_constraints_match_engine_neighbours() {
    let cases: Vec<serde_json::Value> = probe_cases(include_str!(
        "../../../../oracle/data/probes/expr-forms.jsonl"
    ))
    .collect();
    for (id, expected) in [
        ("language/contains-float", "expression-fulltext-language"),
        ("language/contains-repeat", "expression-fulltext-language"),
        ("language/ordinary", "expression-fulltext-language"),
        (
            "language/containstable-top-before",
            "expression-fulltext-language",
        ),
        (
            "language/containstable-top-hex",
            "expression-fulltext-language",
        ),
        ("static/database", "expression-static-reference"),
        ("static/extra-methodVariable", "expression-static-reference"),
        ("static/nullCall", "expression-static-reference"),
        ("set/innerTop", "expression-nested-set-clause"),
        ("provider/numbers", "expression-rowset-provider"),
        ("provider/last", "expression-rowset-provider"),
        ("provider/schema", "expression-rowset-provider"),
    ] {
        let case = cases
            .iter()
            .find(|c| c["id"] == format!("expr-forms/{id}"))
            .unwrap();
        assert_eq!(rules(case["source"].as_str().unwrap()), [expected], "{id}");
    }
    for id in [
        "language/contains-name",
        "language/contains-declared",
        "language/containstable-top-declared",
        "language/qi-comment",
        "language/qi-options",
        "static/schema",
        "static/nullQuoted",
        "set/deep",
        "provider/omitted",
        "provider/table",
    ] {
        let case = cases
            .iter()
            .find(|c| c["id"] == format!("expr-forms/{id}"))
            .unwrap();
        assert!(rules(case["source"].as_str().unwrap()).is_empty(), "{id}");
    }
}

#[test]
fn spaced_comparisons_keep_the_operator_kinds() {
    for op in ["!=", "<=", ">=", "<>", "!<", "!>"] {
        for gap in [
            " ",
            "\r\n",
            "-- 한글\n",
            "/* a /* b */ c */",
            "\u{200b}/*c*/",
        ] {
            let sql = format!("SELECT 1 WHERE 1 {}{gap}{} 2;", &op[..1], &op[1..]);
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_mssql::LANGUAGE.into())
                .unwrap();
            let tree = parser.parse(&sql, None).unwrap();
            assert!(!tree.root_node().has_error(), "{sql}");
            let mut nodes = vec![tree.root_node()];
            let mut found = false;
            while let Some(node) = nodes.pop() {
                if let Some(operator) = node.child_by_field_name("operator") {
                    assert_eq!(operator.kind(), op, "{sql}");
                    assert!(!operator.is_named());
                    found = true;
                    break;
                }
                let mut cursor = node.walk();
                nodes.extend(node.syntax_named_children(&mut cursor));
            }
            assert!(found, "{sql}");
        }
    }
}

const FULL: &str = "PROCEDURE_NAME = dbo.p, MAX_QUEUE_READERS = 1, EXECUTE AS OWNER";

#[test]
fn query_capture_policy_has_known_keys_and_hour_or_day_units() {
    for unit in ["HOURS", "DAYS"] {
        let sql = format!(
            "ALTER DATABASE d SET QUERY_STORE = ON (QUERY_CAPTURE_POLICY = (STALE_CAPTURE_POLICY_THRESHOLD = 5 {unit}));"
        );
        assert!(rules(&sql).is_empty(), "{sql}");
    }
    for option in ["STALE_CAPTURE_POLICY_THRESHOLD = 5 MINUTES", "MYSTERY = 7"] {
        let sql =
            format!("ALTER DATABASE d SET QUERY_STORE = ON (QUERY_CAPTURE_POLICY = ({option}));");
        assert_eq!(rules(&sql), ["query-capture-policy-option"]);
    }
}

#[test]
fn distributed_groups_use_listener_url_not_listener() {
    let prefix = "ALTER AVAILABILITY GROUP a MODIFY AVAILABILITY GROUP ON 'east' WITH";
    assert_eq!(
        rules(&format!("{prefix} (LISTENER = 'TCP://east:5022');")),
        ["distributed-availability-option"]
    );
    assert!(rules(&format!("{prefix} (LISTENER_URL = 'TCP://east:5022');")).is_empty());
}

#[test]
fn remaining_file_attributes_cannot_repeat() {
    for repeated in [
        "FILENAME = 'C:\\b.ndf'",
        "MAXSIZE = 8 MB",
        "FILEGROWTH = 2 MB",
    ] {
        let sql = format!(
            "ALTER DATABASE d ADD FILE (FILENAME = 'C:\\a.ndf', MAXSIZE = 4 MB, FILEGROWTH = 1 MB, {repeated});"
        );
        assert_eq!(rules(&sql), ["database-file-duplicate"], "{sql}");
    }
}

#[test]
fn availability_set_and_modify_have_one_option_per_level() {
    assert_eq!(
        rules("ALTER AVAILABILITY GROUP a SET (DB_FAILOVER = ON, HEALTH_CHECK_TIMEOUT = 30000);"),
        ["availability-set-single-option"]
    );
    assert_eq!(
        rules(
            "ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'n' WITH (SESSION_TIMEOUT = 20, BACKUP_PRIORITY = 40);"
        ),
        ["availability-modify-single-option"]
    );
    assert_eq!(
        rules(
            "ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'n' WITH (PRIMARY_ROLE (ALLOW_CONNECTIONS = ALL, READ_ONLY_ROUTING_LIST = ('b')));"
        ),
        ["availability-modify-role-single-option"]
    );
    assert!(rules("ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'n' WITH (PRIMARY_ROLE (ALLOW_CONNECTIONS = ALL));").is_empty());
    assert!(rules("ALTER AVAILABILITY GROUP a SET (DB_FAILOVER = ON);").is_empty());
}

#[test]
fn availability_required_and_duplicate_options_are_local_to_each_replica() {
    let full = "ENDPOINT_URL = 'TCP://n:5022', AVAILABILITY_MODE = SYNCHRONOUS_COMMIT, FAILOVER_MODE = MANUAL";
    assert_eq!(
        rules(&format!(
            "ALTER AVAILABILITY GROUP a ADD REPLICA ON 'n' WITH ({full}, ENDPOINT_URL = 'TCP://n:5023');"
        )),
        ["availability-replica-duplicate"]
    );
    assert_eq!(
        rules(
            "CREATE AVAILABILITY GROUP a FOR DATABASE d REPLICA ON 'n' WITH (AVAILABILITY_MODE = SYNCHRONOUS_COMMIT, FAILOVER_MODE = MANUAL);"
        ),
        ["availability-replica-required"]
    );
    for role in [
        "PRIMARY_ROLE (READ_ONLY_ROUTING_LIST = ('b'), READ_ONLY_ROUTING_LIST = NONE)",
        "SECONDARY_ROLE (ALLOW_CONNECTIONS = ALL, ALLOW_CONNECTIONS = READ_ONLY)",
    ] {
        let sql = format!("ALTER AVAILABILITY GROUP a ADD REPLICA ON 'n' WITH ({full}, {role});");
        assert_eq!(rules(&sql), ["availability-role-duplicate"], "{sql}");
    }
}

#[test]
fn database_nested_duplicates_do_not_restrict_top_level_repeats() {
    for option in [
        "QUERY_STORE = ON (OPERATION_MODE = READ_WRITE, OPERATION_MODE = READ_ONLY)",
        "QUERY_STORE = ON (CLEANUP_POLICY = (STALE_QUERY_THRESHOLD_DAYS = 4, STALE_QUERY_THRESHOLD_DAYS = 5))",
        "QUERY_STORE = ON (QUERY_CAPTURE_POLICY = (EXECUTION_COUNT = 4, EXECUTION_COUNT = 5))",
        "QUERY_STORE = ON (CLEANUP_POLICY = (STALE_QUERY_THRESHOLD_DAYS = 4), CLEANUP_POLICY = (STALE_QUERY_THRESHOLD_DAYS = 5))",
        "FILESTREAM (DIRECTORY_NAME = 'a', DIRECTORY_NAME = 'b')",
        "CHANGE_TRACKING = ON (AUTO_CLEANUP = ON, AUTO_CLEANUP = OFF)",
    ] {
        let sql = format!("ALTER DATABASE d SET {option};");
        assert_eq!(rules(&sql), ["database-option-duplicate"], "{sql}");
    }
    assert!(
        rules(
            "ALTER DATABASE d SET AUTO_CLOSE ON, AUTO_CLOSE OFF, RECOVERY SIMPLE, RECOVERY FULL;"
        )
        .is_empty()
    );
}

#[test]
fn audit_clauses_cannot_repeat_but_state_within_a_clause_can() {
    for bad in [
        "ALTER SERVER AUDIT a TO SECURITY_LOG TO APPLICATION_LOG;",
        "ALTER SERVER AUDIT a WITH (STATE = OFF) WITH (QUEUE_DELAY = 900);",
    ] {
        assert_eq!(rules(bad), ["server-audit-duplicate-clause"], "{bad}");
    }
    assert!(rules("ALTER SERVER AUDIT a WITH (STATE = OFF, STATE = ON);").is_empty());
}

#[test]
fn buffer_pool_checks_order_duplicates_and_assigned_values() {
    assert_eq!(
        rules(
            "ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON (SIZE = 12 GB, FILENAME = 'C:\\a.bpe');"
        ),
        ["server-buffer-pool-order"]
    );
    assert_eq!(
        rules(
            "ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON (FILENAME = 'C:\\a.bpe', SIZE = 12 GB, SIZE = 16 GB);"
        ),
        ["server-buffer-pool-duplicate"]
    );
    for options in [
        "FILENAME, SIZE = 12 GB",
        "FILENAME = path_name, SIZE = 12 GB",
        "FILENAME = 'C:\\a.bpe', SIZE",
        "FILENAME = 'C:\\a.bpe', SIZE = 12",
        "FILENAME = 'C:\\a.bpe', SIZE = '12' GB",
        "FILENAME = 'C:\\a.bpe', SIZE = 12 TB",
    ] {
        let sql = format!("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON ({options});");
        assert_eq!(rules(&sql), ["server-buffer-pool-value"], "{sql}");
    }
}

#[test]
fn encryption_has_one_assigned_known_algorithm() {
    for options in [
        "ALGORITHM",
        "MYSTERY = AES_256",
        "ALGORITHM = MYSTERY",
        "ALGORITHM = AES_128, ALGORITHM = AES_256",
    ] {
        for sql in [
            format!(
                "CREATE DATABASE ENCRYPTION KEY WITH {options} ENCRYPTION BY SERVER CERTIFICATE c;"
            ),
            format!("ALTER DATABASE ENCRYPTION KEY REGENERATE WITH {options};"),
        ] {
            assert_eq!(rules(&sql), ["database-encryption-algorithm"], "{sql}");
        }
    }
    for algorithm in ["AES_128", "AES_192", "AES_256", "TRIPLE_DES_3KEY"] {
        assert!(rules(&format!("CREATE DATABASE ENCRYPTION KEY WITH ALGORITHM = {algorithm} ENCRYPTION BY SERVER CERTIFICATE c;")).is_empty());
    }
    assert!(rules("ALTER DATABASE ENCRYPTION KEY ENCRYPTION BY SERVER CERTIFICATE c;").is_empty());
}

#[test]
fn database_file_requirements_follow_the_measured_operation() {
    for ok in [
        "ALTER DATABASE d ADD FILE (FILENAME = 'C:\\probe.ndf');",
        "ALTER DATABASE d MODIFY FILE (SIZE = 8 MB);",
        "CREATE DATABASE d ON (FILENAME = 'C:\\probe.mdf') FOR ATTACH;",
        "CREATE DATABASE d ON (NAME = f, FILENAME = 'C:\\probe.mdf');",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "ALTER DATABASE d ADD FILE (NAME = f);",
        "CREATE DATABASE d ON (NAME = f);",
    ] {
        assert_eq!(rules(bad), ["database-file-required"], "{bad}");
    }
}

#[test]
fn measured_file_duplicates_are_rejected_case_insensitively() {
    for bad in [
        "ALTER DATABASE d ADD FILE (NAME = f, name = g, FILENAME = 'C:\\probe.ndf');",
        "ALTER DATABASE d ADD FILE (FILENAME = 'C:\\probe.ndf', SIZE = 1 MB, size = 2 MB);",
        "ALTER DATABASE d MODIFY FILE (NAME = f, OFFLINE, offline);",
    ] {
        assert_eq!(rules(bad), ["database-file-duplicate"], "{bad}");
    }
    assert!(rules("ALTER DATABASE d ADD FILE (NAME = f, FILENAME = 'C:\\a.ndf'), (NAME = g, FILENAME = 'C:\\b.ndf');").is_empty());
}

#[test]
fn add_replica_requires_each_measured_option_but_modify_does_not() {
    let full = [
        "ENDPOINT_URL = 'TCP://probe:5022'",
        "AVAILABILITY_MODE = SYNCHRONOUS_COMMIT",
        "FAILOVER_MODE = MANUAL",
    ];
    for missing in 0..full.len() {
        let options = full
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != missing)
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("ALTER AVAILABILITY GROUP a ADD REPLICA ON 'probe' WITH ({options});");
        assert_eq!(rules(&sql), ["availability-replica-required"], "{sql}");
    }
    assert!(
        rules(&format!(
            "ALTER AVAILABILITY GROUP a ADD REPLICA ON 'probe' WITH ({});",
            full.join(", ")
        ))
        .is_empty()
    );
    assert!(
        rules(
            "ALTER AVAILABILITY GROUP a MODIFY REPLICA ON 'probe' WITH (FAILOVER_MODE = MANUAL);"
        )
        .is_empty()
    );
}

#[test]
fn buffer_pool_extension_requires_filename_and_size() {
    for options in ["FILENAME = 'C:\\probe.bpe'", "SIZE = 12 GB"] {
        let sql = format!("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON ({options});");
        assert_eq!(rules(&sql), ["server-buffer-pool-required"], "{sql}");
    }
    assert!(rules("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION ON (FILENAME = 'C:\\probe.bpe', SIZE = 12 GB);").is_empty());
    assert!(rules("ALTER SERVER CONFIGURATION SET BUFFER POOL EXTENSION OFF;").is_empty());
}

#[test]
fn complete_activation_in_any_order_passes() {
    assert!(rules(&format!("CREATE QUEUE q WITH ACTIVATION ({FULL});")).is_empty());
    let reordered = "EXECUTE AS SELF, STATUS = ON, MAX_QUEUE_READERS = 2, PROCEDURE_NAME = p";
    assert!(rules(&format!("CREATE QUEUE q WITH ACTIVATION ({reordered});")).is_empty());
}

#[test]
fn create_needs_the_three_required_settings() {
    let missing =
        "CREATE QUEUE q WITH ACTIVATION (MAX_QUEUE_READERS = 1, EXECUTE AS OWNER, STATUS = OFF);";
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
fn exec_output_parameters_need_at() {
    let q = "N'SELECT ?'";
    assert!(rules(&format!("DECLARE @v int; EXEC({q}, @v OUTPUT) AT [lnk];")).is_empty());
    assert!(rules(&format!("DECLARE @v int; EXEC({q}, @v OUT) AT lnk;")).is_empty());
    assert_eq!(
        rules(&format!("DECLARE @v int; EXEC({q}, @v OUTPUT);")),
        ["exec-output-requires-at"]
    );
    assert!(rules("DECLARE @v int; EXEC dbo.p @v OUTPUT;").is_empty());
}

#[test]
fn output_marks_only_a_local_variable() {
    for ok in [
        "EXEC dbo.p @v OUTPUT;",
        "EXEC dbo.p @v OUT;",
        "EXEC dbo.p @a = @v OUTPUT;",
        "EXEC(N'x', @v OUTPUT) AT l;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "EXEC dbo.p 1 OUTPUT;",
        "EXEC dbo.p 'a' OUTPUT;",
        "EXEC dbo.p DEFAULT OUTPUT;",
        "EXEC dbo.p @@ROWCOUNT OUTPUT;",
        "EXEC dbo.p x OUTPUT;",
        "EXEC dbo.p @a = 1 OUTPUT;",
        "EXEC(N'x', 1 OUTPUT) AT l;",
        "EXEC(N'x', @@ROWCOUNT OUTPUT) AT l;",
    ] {
        assert_eq!(rules(bad), ["exec-output-variable"], "{bad}");
    }
}

#[test]
fn column_lists_allow_only_some_permissions() {
    for ok in [
        "GRANT SELECT ON dbo.t (a) TO public;",
        "GRANT SELECT (a), INSERT ON dbo.t TO public;",
        "DENY UNMASK ON dbo.t (a) TO public;",
        "GRANT ALL PRIVILEGES ON dbo.t (a) TO public;",
        "GRANT DELETE ON dbo.t TO public;",
        "REVOKE GRANT OPTION FOR SELECT ON dbo.t (a) FROM public CASCADE;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "GRANT INSERT ON dbo.t (a) TO public;",
        "GRANT SELECT, INSERT (a) ON dbo.t TO public;",
        "REVOKE DELETE ON OBJECT::dbo.t (a) FROM public;",
        "DENY VIEW DEFINITION (a) ON dbo.t TO public;",
    ] {
        assert_eq!(rules(bad), ["column-permission"], "{bad}");
    }
}

#[test]
fn triggers_need_a_body() {
    for sql in [
        "CREATE TRIGGER dbo.tr ON dbo.t AFTER INSERT AS",
        "ALTER TRIGGER dbo.tr ON dbo.t AFTER INSERT AS /* comment */",
        "CREATE TRIGGER dbo.tr ON dbo.t AFTER INSERT AS ;",
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert_eq!(rules(sql), ["trigger-body"], "{sql}");
    }
    for sql in [
        "CREATE PROCEDURE dbo.p AS",
        "CREATE TRIGGER dbo.tr ON dbo.t AFTER INSERT AS SELECT 1;",
        "ALTER TRIGGER dbo.tr ON dbo.t AFTER INSERT AS EXTERNAL NAME a.b.c;",
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert!(rules(sql).is_empty(), "{sql}");
    }
}

#[test]
fn certificates_require_a_supported_source() {
    for sql in [
        "CREATE CERTIFICATE c FROM /* comment */ PROVIDER p;",
        "CREATE CERTIFICATE c FROM [PROVIDER];",
        "CREATE CERTIFICATE c FROM [provider] = 'p';",
        r#"CREATE CERTIFICATE c FROM "PROVIDER" = 'p';"#,
        "CREATE CERTIFICATE c FROM [PROVIDER ] = 'p';",
        "CREATE CERTIFICATE c FROM [FILE] = 'c.cer';",
        "CREATE CERTIFICATE c FROM FILE 'c.cer';",
        "CREATE CERTIFICATE c FROM BINARY 0x3082010A;",
        "CREATE CERTIFICATE c FROM OTHER = 'c.cer';",
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert_eq!(rules(sql), ["certificate-source"], "{sql}");
    }

    for sql in [
        "CREATE CERTIFICATE [provider] FROM FILE = 'c.cer';",
        "CREATE CERTIFICATE c FROM EXECUTABLE FILE = 'c.dll';",
        "CREATE CERTIFICATE c FROM BINARY = 0x3082010A;",
        "CREATE CERTIFICATE c FROM ASSEMBLY [asm];",
        "CREATE CERTIFICATE c WITH SUBJECT = 'x';",
        "CREATE ASYMMETRIC KEY k FROM PROVIDER p WITH PROVIDER_KEY_NAME = 'x', CREATION_DISPOSITION = OPEN_EXISTING;",
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert!(rules(sql).is_empty(), "{sql}");
    }
}

#[test]
fn tvf_return_shapes_require_matching_bodies() {
    for verb in ["CREATE", "ALTER"] {
        for tail in [
            "TABLE (a int) AS RETURN SELECT 1 AS a;",
            "TABLE /* comment */ (a int) AS BEGIN RETURN; END;",
            "TABLE AS EXTERNAL NAME a.b.c;",
            "TABLE AS BEGIN RETURN; END;",
            "@r TABLE (a int) AS EXTERNAL NAME a.b.c;",
            "@r TABLE (a int) AS RETURN SELECT 1 AS a;",
        ] {
            let sql = format!("{verb} FUNCTION dbo.f() RETURNS {tail}");
            assert!(!parse(&sql).root_node().has_error(), "{sql}");
            assert_eq!(rules(&sql), ["function-return-body"], "{sql}");
        }
        for tail in [
            "TABLE AS RETURN SELECT 1 AS a;",
            "TABLE (a int) AS EXTERNAL NAME a.b.c;",
            "@r TABLE (a int) AS BEGIN RETURN; END;",
        ] {
            let sql = format!("{verb} FUNCTION dbo.f() RETURNS {tail}");
            assert!(!parse(&sql).root_node().has_error(), "{sql}");
            assert!(rules(&sql).is_empty(), "{sql}");
        }
    }
}

#[test]
fn scalar_functions_require_block_or_external_bodies() {
    for sql in [
        "CREATE FUNCTION dbo.f() RETURNS int AS RETURN SELECT 1;",
        "CREATE OR ALTER FUNCTION dbo.f() RETURNS int WITH SCHEMABINDING RETURN (SELECT 1);",
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert_eq!(rules(sql), ["function-return-body"], "{sql}");
    }
    for sql in [
        "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN 1; END;",
        "CREATE OR ALTER FUNCTION dbo.f() RETURNS int WITH SCHEMABINDING AS BEGIN RETURN 1; END;",
        "CREATE FUNCTION dbo.f() RETURNS int AS EXTERNAL NAME a.b.c;",
    ] {
        assert!(!parse(sql).root_node().has_error(), "{sql}");
        assert!(rules(sql).is_empty(), "{sql}");
    }
}

#[test]
fn create_or_alter_tvf_checks_the_body_after_options() {
    let header =
        "CREATE OR ALTER FUNCTION dbo.f() RETURNS @r TABLE (x int) WITH EXECUTE AS CALLER AS";
    let external = format!("{header} EXTERNAL NAME a.b.c;");
    assert!(!parse(&external).root_node().has_error());
    assert_eq!(rules(&external), ["function-return-body"]);

    let block = format!("{header} BEGIN RETURN; END;");
    assert!(!parse(&block).root_node().has_error());
    assert!(rules(&block).is_empty());
}

#[test]
fn trigger_events_match_the_scope() {
    for ok in [
        "CREATE TRIGGER tr ON DATABASE FOR CREATE_TABLE AS RETURN;",
        "CREATE TRIGGER tr ON ALL SERVER FOR CREATE_LOGIN AS RETURN;",
        "CREATE TRIGGER dbo.tr ON dbo.t WITH EXECUTE AS OWNER FOR INSERT, UPDATE AS RETURN;",
        "CREATE TRIGGER tr ON DATABASE WITH EXECUTE AS SELF FOR DDL_TABLE_EVENTS AS RETURN;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "CREATE TRIGGER dbo.tr ON DATABASE FOR CREATE_TABLE AS RETURN;",
        "ALTER TRIGGER [dbo].[tr] ON ALL SERVER FOR CREATE_TABLE AS RETURN;",
        "CREATE TRIGGER tr ON DATABASE FOR INSERT AS RETURN;",
        "CREATE TRIGGER tr ON dbo.t FOR CREATE_TABLE AS RETURN;",
        "CREATE TRIGGER tr ON DATABASE FOR DDL_SERVER_LEVEL_EVENTS AS RETURN;",
        "CREATE TRIGGER tr ON ALL SERVER FOR ALTER_SERVER_CONFIGURATION AS RETURN;",
        "CREATE OR ALTER TRIGGER tr ON DATABASE WITH EXECUTE AS OWNER FOR CREATE_TABLE AS RETURN;",
    ] {
        assert_eq!(rules(bad), ["trigger-scope"], "{bad}");
    }
}

#[test]
fn fixed_arity_builtins_count_arguments() {
    for ok in [
        "SELECT COUNT(*), COUNT_BIG(DISTINCT a), SUM(a), ISNULL(a, 0) FROM dbo.t;",
        "SELECT STRING_AGG(a, ',') FROM dbo.t;",
        "SELECT dbo.COUNT(a, b), dbo.SUM() FROM dbo.t;",
        "SELECT GROUPING_ID(a, b) FROM dbo.t GROUP BY a, b;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "SELECT COUNT();",
        "SELECT count(a, b) FROM dbo.t;",
        "SELECT MAX() FROM dbo.t;",
        "SELECT ISNULL(a) FROM dbo.t;",
        "SELECT COUNT() OVER () FROM dbo.t;",
    ] {
        assert_eq!(rules(bad), ["builtin-arity"], "{bad}");
    }
}

#[test]
fn scalar_functions_return_a_value() {
    for ok in [
        "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN 1; END;",
        "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN (1); END;",
        "CREATE FUNCTION dbo.f() RETURNS @r TABLE (a int) AS BEGIN RETURN; END;",
        "CREATE FUNCTION dbo.f() RETURNS TABLE AS RETURN SELECT 1 AS a;",
        "CREATE PROCEDURE dbo.p AS BEGIN RETURN; END;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "CREATE FUNCTION dbo.f() RETURNS int AS BEGIN RETURN; END;",
        "ALTER FUNCTION dbo.f() RETURNS int AS BEGIN IF 1=1 RETURN; RETURN 1; END;",
    ] {
        assert_eq!(rules(bad), ["scalar-function-return"], "{bad}");
    }
}

#[test]
fn a_cte_query_needs_a_table_source() {
    for ok in [
        "WITH c AS (SELECT 1 AS a) SELECT a FROM c;",
        "WITH c AS (SELECT 1 AS a) SELECT (SELECT a FROM c);",
        "WITH c AS (SELECT 1 AS a) SELECT 1 WHERE EXISTS (SELECT 1 FROM c);",
        "WITH c AS (SELECT 1 AS a) SELECT 1 UNION SELECT 2;",
        "WITH c AS (SELECT 1 AS a) UPDATE dbo.t SET a = 1;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "WITH c AS (SELECT 1 AS a) SELECT 1;",
        "WITH c AS (SELECT 1 AS a), d AS (SELECT a FROM c) SELECT 1;",
    ] {
        assert_eq!(rules(bad), ["cte-table-source"], "{bad}");
    }
}

#[test]
fn a_module_starts_its_batch_and_some_end_it() {
    for ok in [
        "CREATE PROCEDURE p AS SELECT 1;\nSELECT 2;",
        "-- header\n;CREATE VIEW v AS SELECT 1 AS a;\n-- end",
        "SELECT 1\nGO\nCREATE FUNCTION f() RETURNS int AS BEGIN RETURN 1 END;",
        "CREATE TRIGGER tr ON t AFTER INSERT AS SELECT 1;\nSELECT 2;",
        "CREATE TRIGGER tr ON t AFTER INSERT AS EXTERNAL NAME a.b.c;\nGO\nSELECT 1;",
        "CREATE PROCEDURE p AS EXTERNAL NAME a.b.c;\nGO\nSELECT 1;",
        "CREATE FUNCTION f() RETURNS int AS EXTERNAL NAME a.b.c;\nGO\nSELECT 1;",
        "CREATE TRIGGER tr ON t AFTER INSERT AS EXTERNAL NAME a.b.c;\nGO\nBEGIN SELECT 1; END;",
        "BEGIN SELECT 1; END;\nGO\nCREATE FUNCTION f() RETURNS int AS EXTERNAL NAME a.b.c;",
        "CREATE TRIGGER tr ON t AFTER INSERT AS BEGIN SELECT 1; END;",
        "CREATE SCHEMA x CREATE VIEW v AS SELECT 1 AS a;",
        "SELECT 1;\nCREATE TABLE t (a int);",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "SELECT 1;\nCREATE PROCEDURE p AS SELECT 1;",
        "SET NOCOUNT ON\nALTER VIEW v AS SELECT 1 AS a;",
        "CREATE VIEW v AS SELECT 1 AS a;\nSELECT 2;",
        "CREATE FUNCTION f() RETURNS TABLE AS RETURN SELECT 1 AS a;\nSELECT 2;",
        "CREATE TRIGGER tr ON t AFTER INSERT AS EXTERNAL NAME a.b.c; SELECT 1;",
        "ALTER TRIGGER tr ON t AFTER INSERT AS EXTERNAL NAME a.b.c; SELECT 1;",
        "CREATE PROCEDURE p AS EXTERNAL NAME a.b.c; SELECT 1;",
        "ALTER PROCEDURE p AS EXTERNAL NAME a.b.c; SELECT 1;",
        "CREATE FUNCTION f() RETURNS int AS EXTERNAL NAME a.b.c; SELECT 1;",
        "CREATE TRIGGER tr ON t AFTER INSERT AS EXTERNAL NAME a.b.c; BEGIN SELECT 1; END;",
        "ALTER PROC p AS EXTERNAL NAME a.b.c; BEGIN SELECT 1; END;",
        "CREATE FUNCTION f() RETURNS int AS EXTERNAL NAME a.b.c; BEGIN SELECT 1; END;",
        "BEGIN SELECT 1; END; CREATE FUNCTION f() RETURNS int AS EXTERNAL NAME a.b.c;",
        "IF 1 = 1 CREATE PROCEDURE p AS SELECT 1;",
        "CREATE PROCEDURE p AS CREATE VIEW v AS SELECT 1 AS a;",
        "SELECT 1;\nCREATE RULE r AS @x > 0;",
    ] {
        assert!(!parse(bad).root_node().has_error(), "{bad}");
        assert_eq!(rules(bad), ["batch-module"], "{bad}");
    }
}

#[test]
fn a_go_line_inside_a_comment_or_string_splits_the_batch() {
    for ok in [
        "SELECT 1\nGO\nSELECT 2;",
        "-- GO\nSELECT 1;",
        "/* GO is a client command */\nSELECT 1;",
        "SELECT 1;\n  go 2 -- twice\n",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "/*\nGO\n*/\nSELECT 1;",
        "SELECT 1;\n/* note\n  go 2\n*/\nSELECT 2;",
        "SELECT '\nGO\n';",
    ] {
        assert_eq!(rules(bad), ["go-inside-token"], "{bad}");
    }
}

#[test]
fn a_table_variable_or_type_has_no_foreign_key() {
    for ok in [
        "DECLARE @t TABLE (a int CHECK (a > 0), b int PRIMARY KEY (b));",
        "CREATE TABLE #t (a int FOREIGN KEY (a) REFERENCES r (a));",
        "CREATE TABLE t (a int, b int NOT NULL CONSTRAINT f FOREIGN KEY (a) REFERENCES r (a));",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "DECLARE @t TABLE (a int REFERENCES r (a));",
        "DECLARE @t TABLE (a int, FOREIGN KEY (a) REFERENCES r (a));",
        "CREATE TYPE tt AS TABLE (a int FOREIGN KEY (a) REFERENCES r (a));",
        "CREATE FUNCTION f() RETURNS @t TABLE (a int REFERENCES r (a)) AS BEGIN RETURN END;",
    ] {
        assert_eq!(rules(bad), ["table-variable-foreign-key"], "{bad}");
    }
}

#[test]
fn a_window_frame_needs_order_by() {
    for ok in [
        "SELECT MAX(a) OVER (ORDER BY a ROWS 1 PRECEDING) FROM dbo.t;",
        "SELECT MAX(a) OVER (PARTITION BY a) FROM dbo.t;",
        "SELECT MAX(a) OVER (w ROWS 1 PRECEDING) FROM dbo.t WINDOW w AS (ORDER BY a);",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "SELECT MAX(a) OVER (PARTITION BY a RANGE UNBOUNDED PRECEDING) FROM dbo.t;",
        "SELECT MAX(a) OVER (PARTITION BY (SELECT 1 ORDER BY (SELECT 1) OFFSET 0 ROWS) ROWS 1 PRECEDING) FROM dbo.t;",
    ] {
        assert_eq!(rules(bad), ["window-frame-order"], "{bad}");
    }
}

#[test]
fn merge_ends_with_a_semicolon() {
    const M: &str = "MERGE dbo.t USING dbo.s ON 1=1 WHEN MATCHED THEN DELETE";
    for ok in [
        format!("{M};"),
        format!("BEGIN {M}; END;"),
        format!("IF 1=1 {M}; ELSE SELECT 1;"),
        format!("{M} /* c */ ;"),
        format!("{M} OUTPUT $action;"),
        format!("INSERT INTO dbo.u SELECT a FROM ({M} OUTPUT deleted.a) AS x (a);"),
    ] {
        assert!(rules(&ok).is_empty(), "{ok}");
    }
    for bad in [
        M.to_string(),
        format!("BEGIN {M} END;"),
        format!("IF 1=1 {M} ELSE SELECT 1;"),
        format!("CREATE PROCEDURE dbo.p AS {M}"),
    ] {
        assert_eq!(rules(&bad), ["merge-terminator"], "{bad}");
    }
}

#[test]
fn table_hints_follow_the_engine_matrix() {
    for ok in [
        "SELECT a FROM dbo.t WITH (NOLOCK, READUNCOMMITTED, NOWAIT);",
        "SELECT a FROM dbo.t WITH (SERIALIZABLE HOLDLOCK ROWLOCK UPDLOCK);",
        "SELECT a FROM dbo.t WITH (nolock, index(ix));",
        "SELECT a FROM dbo.t WITH (FORCESEEK, INDEX(ix), INDEX(0));",
        "SELECT a FROM dbo.t WITH (FORCESEEK(1(a)));",
        "SELECT a FROM dbo.t WITH (NOLOCK) JOIN dbo.s WITH (UPDLOCK) ON 1 = 1;",
        "SELECT a FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK), TABLE HINT (y, UPDLOCK));",
        "UPDATE x SET a = 1 FROM dbo.t AS x WITH (NOLOCK);",
        "UPDATE x SET a = 1 FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK));",
        "MERGE dbo.t WITH (INDEX(ix), FORCESCAN) AS t USING dbo.s AS s ON 1 = 1 WHEN MATCHED THEN DELETE;",
        "INSERT dbo.t WITH (TABLOCK) SELECT a FROM dbo.s WITH (NOLOCK);",
        "UPDATE dbo.t WITH (READPAST) SET a = 1;",
        "DECLARE @j nvarchar(max); SELECT * FROM OPENJSON(@j) WITH (ROWLOCK int);",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "SELECT a FROM dbo.t WITH (NOLOCK, HOLDLOCK);",
        "SELECT a FROM dbo.t WITH (READUNCOMMITTED UPDLOCK);",
        "SELECT a FROM dbo.t WITH (ROWLOCK, PAGLOCK);",
        "SELECT a FROM dbo.t WITH (FORCESEEK, FORCESCAN);",
        "SELECT a FROM dbo.t WITH (FORCESEEK(ix(a)), INDEX(ix));",
        "SELECT a FROM dbo.t WITH (FORCESEEK(ix(a)), FORCESEEK);",
        "SELECT a FROM dbo.t WITH (FORCESEEK(00(a)));",
        "SELECT a FROM dbo.t WITH (SPATIAL_WINDOW_MAX_CELLS=8, SPATIAL_WINDOW_MAX_CELLS=8);",
        "SELECT a FROM dbo.t WITH (NOLOCKX);",
        "SELECT a FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK, UPDLOCK));",
        "SELECT a FROM dbo.t AS x OPTION (TABLE HINT (x, NOLOCK), TABLE HINT (x, UPDLOCK));",
        "UPDATE dbo.t WITH (NOLOCK) SET a = 1;",
        "DELETE FROM dbo.t WITH (FORCESCAN);",
        "INSERT INTO dbo.t WITH (READPAST) (a) VALUES (1);",
        "MERGE dbo.t WITH (READUNCOMMITTED) AS t USING dbo.s AS s ON 1 = 1 WHEN MATCHED THEN DELETE;",
    ] {
        assert_eq!(rules(bad), ["table-hint"], "{bad}");
    }
}

#[test]
fn conditions_are_not_scalars() {
    for ok in [
        "SELECT a FROM t JOIN u ON t.a = u.a WHERE a IS NOT NULL AND b NOT LIKE 'x' OR NOT (c IN (1));",
        "SELECT a FROM t GROUP BY a HAVING COUNT(*) > 1;",
        "SELECT CASE WHEN a = 1 THEN 1 END, CASE a WHEN 1 THEN 2 END, IIF(a > 1, 1, 0) FROM t;",
        "SELECT 1 WHERE EXISTS (SELECT 1) AND 1 BETWEEN 0 AND 2 AND a IS DISTINCT FROM b;",
        "IF (@x = 1) PRINT 1;",
        "WHILE NOT (1 = 0) BREAK;",
        "MERGE dbo.t USING dbo.s ON (t.id = s.id) WHEN MATCHED AND s.a > 0 THEN DELETE;",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for bad in [
        "IF 1 PRINT 1;",
        "IF NOT 1 PRINT 1;",
        "WHILE @x BREAK;",
        "SELECT a FROM t WHERE a;",
        "SELECT a FROM t WHERE a = 1 AND b;",
        "SELECT a FROM t GROUP BY a HAVING COUNT(*);",
        "SELECT CASE WHEN a THEN 1 END FROM t;",
        "SELECT IIF(a, 1, 0) FROM t;",
        "MERGE dbo.t USING dbo.s ON t.id + 1 WHEN MATCHED THEN DELETE;",
        "MERGE dbo.t USING dbo.s ON (t.id) WHEN MATCHED THEN DELETE;",
    ] {
        assert_eq!(rules(bad), ["scalar-predicate"], "{bad}");
    }
}

#[test]
fn other_statements_are_not_checked() {
    assert!(rules("SELECT 1 WHERE 1 = 1;").is_empty());
}

#[test]
fn external_option_sets_match_the_engine() {
    for sql in [
        "CREATE EXTERNAL LANGUAGE Java FROM (FILE_NAME = 'java.so', CONTENT = 0x504B0304);",
        "ALTER EXTERNAL LANGUAGE Java SET (FILE_NAME = 'java.so');",
        "ALTER EXTERNAL LANGUAGE Java REMOVE PLATFORM LINUX;",
        "CREATE EXTERNAL MODEL emb WITH (API_FORMAT = 'OpenAI', MODEL_TYPE = EMBEDDINGS, MODEL = 'm');",
        "ALTER EXTERNAL MODEL emb SET (PARAMETERS = '{}');",
        "CREATE EXTERNAL DATA SOURCE ds WITH (CREDENTIAL = cred);",
        "ALTER EXTERNAL LIBRARY pkg SET (PLATFORM = LINUX) WITH (LANGUAGE = 'Python');",
        "ALTER EXTERNAL RESOURCE POOL ep WITH (AFFINITY NUMANODE = (0 TO 1));",
    ] {
        assert!(rules(sql).is_empty(), "{sql}");
    }
    for (sql, rule) in [
        (
            "ALTER EXTERNAL RESOURCE POOL ep WITH (AFFINITY NUMANODE = AUTO);",
            "external-affinity",
        ),
        (
            "ALTER EXTERNAL RESOURCE POOL ep WITH (AFFINITY CPU = UNKNOWN_VALUE);",
            "external-affinity",
        ),
        (
            "CREATE EXTERNAL LANGUAGE Java FROM (FILE_NAME = 'java.so');",
            "external-option-required",
        ),
        (
            "CREATE EXTERNAL LANGUAGE Java FROM (CONTENT = 0x504B0304);",
            "external-option-required",
        ),
        (
            "CREATE EXTERNAL LANGUAGE Java FROM (CONTENT = 0x504B0304, FILE_NAME = 'java.so', FILE_NAME = 'java2.so');",
            "external-option-duplicate",
        ),
        (
            "CREATE EXTERNAL LIBRARY pkg FROM (PLATFORM = LINUX) WITH (LANGUAGE = 'Python');",
            "external-option-required",
        ),
        (
            "CREATE EXTERNAL LIBRARY pkg FROM (CONTENT = 0x504B0304, CONTENT = 0x504B0304) WITH (LANGUAGE = 'Python');",
            "external-option-duplicate",
        ),
        (
            "ALTER EXTERNAL LANGUAGE Java REMOVE PLATFORM MACOS;",
            "external-platform",
        ),
        (
            "ALTER EXTERNAL DATA SOURCE ds SET LOCATION = 'abs://a', LOCATION = 'abs://b';",
            "external-option-duplicate",
        ),
        (
            "CREATE EXTERNAL DATA SOURCE ds WITH (LOCATION = 'abs://a', LOCATION = 'abs://b');",
            "external-option-duplicate",
        ),
        (
            "ALTER EXTERNAL DATA SOURCE ds SET UNKNOWN_OPTION = 1;",
            "external-option",
        ),
        (
            "ALTER EXTERNAL RESOURCE POOL ep WITH (MAX_CPU_PERCENT = 10, MAX_CPU_PERCENT = 20);",
            "external-option-duplicate",
        ),
        (
            "CREATE EXTERNAL RESOURCE POOL ep WITH (UNKNOWN_OPTION = 1);",
            "external-option",
        ),
        (
            "CREATE EXTERNAL MODEL emb WITH (MODEL = 'm', MODEL = 'm2');",
            "external-option-duplicate",
        ),
    ] {
        assert_eq!(rules(sql), [rule], "{sql}");
    }
}
#[test]
fn fulltext_options_appear_at_most_once() {
    for options in [
        "CHANGE_TRACKING AUTO, CHANGE_TRACKING MANUAL",
        "STOPLIST SYSTEM, STOPLIST OFF",
        "SEARCH PROPERTY LIST = spl, SEARCH PROPERTY LIST = spl2",
    ] {
        assert_eq!(
            rules(&format!(
                "CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH ({options});"
            )),
            ["fulltext-option-duplicate"],
            "{options}"
        );
    }
    assert_eq!(
        rules(
            "CREATE FULLTEXT CATALOG ftc WITH ACCENT_SENSITIVITY = ON, ACCENT_SENSITIVITY = OFF;"
        ),
        ["fulltext-option-duplicate"]
    );
    assert!(rules("CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH (CHANGE_TRACKING AUTO, STOPLIST SYSTEM, SEARCH PROPERTY LIST = spl);").is_empty());
}

#[test]
fn fulltext_change_tracking_has_a_mode() {
    for mode in ["AUTO", "MANUAL", "OFF"] {
        for sql in [
            format!(
                "CREATE FULLTEXT INDEX ON dbo.docs KEY INDEX pk WITH (CHANGE_TRACKING = {mode});"
            ),
            format!("CREATE FULLTEXT INDEX ON dbo.docs KEY INDEX pk WITH CHANGE_TRACKING {mode};"),
        ] {
            assert!(rules(&sql).is_empty(), "{sql}");
        }
    }
    assert_eq!(
        rules(
            "CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH (CHANGE_TRACKING BOGUS);"
        ),
        ["fulltext-change-tracking"]
    );
}

#[test]
fn fulltext_no_population_requires_tracking_off_in_create() {
    for options in ["NO POPULATION", "CHANGE_TRACKING AUTO, NO POPULATION"] {
        assert_eq!(
            rules(&format!(
                "CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH ({options});"
            )),
            ["fulltext-no-population"],
            "{options}"
        );
    }
    assert!(rules("CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH (CHANGE_TRACKING OFF, NO POPULATION, STOPLIST sl);").is_empty());
    assert!(rules("CREATE FULLTEXT INDEX ON dbo.docs (body) KEY INDEX pk WITH change_tracking /*mode*/ off, no /*skip*/ population;").is_empty());
    assert!(
        rules("ALTER FULLTEXT INDEX ON dbo.docs SET STOPLIST SYSTEM WITH NO POPULATION;")
            .is_empty()
    );
}
#[test]
fn endpoint_sets_match_the_engine() {
    for ok in [
        "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = ALL) FOR TSQL ();",
        "CREATE ENDPOINT ep AS TCP (LISTENER_IP = ALL, LISTENER_PORT = 1434) FOR TSQL (ENCRYPTION = REQUIRED);",
        "ALTER ENDPOINT ep STATE = STOPPED;",
        "ALTER ENDPOINT ep AS TCP (LISTENER_IP = ALL);",
        "ALTER ENDPOINT ep FOR DATABASE_MIRRORING ();",
        "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ENCRYPTION = REQUIRED);",
        "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = STRICT);",
        "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = WINDOWS NTLM CERTIFICATE c, ENCRYPTION = REQUIRED ALGORITHM AES RC4);",
        "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = CERTIFICATE c WINDOWS NEGOTIATE, ENCRYPTION = SUPPORTED ALGORITHM RC4 AES);",
        "ALTER ENDPOINT ep FOR SERVICE_BROKER (MESSAGE_FORWARD_SIZE = 10, MESSAGE_FORWARDING = ENABLED, ENCRYPTION = REQUIRED);",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for (bad, rule) in [
        (
            "ALTER ENDPOINT ep FOR TSQL (ENCRYPTION = BOGUS);",
            "endpoint-value",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = BOGUS);",
            "endpoint-value",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = (10)) FOR TSQL ();",
            "endpoint-value",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = (10.0)) FOR TSQL ();",
            "endpoint-value",
        ),
        (
            "CREATE ENDPOINT ep STATE = DISABLED AS TCP (LISTENER_PORT = 1436) FOR TSQL ();",
            "endpoint-create-state",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = (999.0.0.1)) FOR TSQL ();",
            "endpoint-value",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_IP = ALL) FOR TSQL ();",
            "endpoint-required",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 5022) FOR DATABASE_MIRRORING (ENCRYPTION = REQUIRED);",
            "endpoint-required",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_PORT = 1435) FOR TSQL ();",
            "endpoint-duplicate",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, LISTENER_IP = ALL, LISTENER_IP = ALL) FOR TSQL ();",
            "endpoint-duplicate",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = REQUIRED, ENCRYPTION = SUPPORTED);",
            "endpoint-duplicate",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = WINDOWS, AUTHENTICATION = CERTIFICATE c);",
            "endpoint-duplicate",
        ),
        (
            "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ROLE = WITNESS, ROLE = ALL);",
            "endpoint-duplicate",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (AUTHENTICATION = WINDOWS BASIC);",
            "endpoint-authentication",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = REQUIRED ALGORITHM DES);",
            "endpoint-algorithm",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = DISABLED ALGORITHM AES);",
            "endpoint-algorithm",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ENCRYPTION = REQUIRED ALGORITHM AES AES);",
            "endpoint-algorithm",
        ),
        (
            "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ROLE = LEADER);",
            "endpoint-value",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (MESSAGE_FORWARDING = ON);",
            "endpoint-value",
        ),
        (
            "CREATE ENDPOINT ep AS TCP (LISTENER_PORT = 1434, UNKNOWN_OPTION = 1) FOR TSQL ();",
            "endpoint-option",
        ),
        (
            "ALTER ENDPOINT ep FOR SERVICE_BROKER (ROLE = WITNESS);",
            "endpoint-option",
        ),
        (
            "ALTER ENDPOINT ep FOR DATABASE_MIRRORING (ROLE = WITNESS, MESSAGE_FORWARDING = ENABLED);",
            "endpoint-option",
        ),
        (
            "ALTER ENDPOINT ep FOR TSQL (AUTHENTICATION = WINDOWS);",
            "endpoint-option",
        ),
    ] {
        assert_eq!(rules(bad), [rule], "{bad}");
    }
}

#[test]
fn query_forms_constraints_match_engine_boundaries() {
    let cases: Vec<serde_json::Value> = probe_cases(include_str!(
        "../../../../oracle/data/probes/query-forms.jsonl"
    ))
    .collect();
    let source = |id: &str| {
        cases
            .iter()
            .find(|c| c["id"] == format!("query-forms/{id}"))
            .unwrap()["source"]
            .as_str()
            .unwrap()
    };
    for (id, expected) in [
        ("select-distinct-all", "query-select-quantifier"),
        ("function-cursor-param", "query-cursor-parameter"),
        ("function-native-returns-null", "query-native-null-input"),
        ("offsets-bogus", "query-offsets-options"),
        ("cast-using", "query-cast-using"),
        ("try-cast-using", "query-cast-using"),
        ("cursor-iso", "query-cursor-options"),
        ("cursor-iso-after", "query-cursor-options"),
        ("cursor-bogus", "query-cursor-options"),
        ("cursor-two-scope", "query-cursor-options"),
        ("cursor-fast-scroll", "query-cursor-options"),
        ("cursor-read-optimistic", "query-cursor-options"),
        ("cursor-static-keyset", "query-cursor-options"),
        ("update-column-chain", "query-assignment-chain"),
        ("select-delete-source", "query-dml-source"),
        ("insert-delete-no-output", "query-dml-source"),
        ("openjson-not-null", "query-rowset-schema"),
        ("openxml-not-null", "query-rowset-schema"),
        ("openjson-table-schema", "query-rowset-schema"),
        ("exec-repeat-recompile", "query-exec-options"),
        ("exec-repeat-results", "query-exec-options"),
        ("plain-atomic", "query-atomic-options"),
        ("function-native-missing-atomic", "query-native-atomic"),
        ("atomic-no-language", "query-atomic-options"),
        ("atomic-no-isolation", "query-atomic-options"),
        ("atomic-repeat-language", "query-atomic-options"),
        ("atomic-repeat-isolation", "query-atomic-options"),
        ("atomic-unknown", "query-atomic-options"),
        ("atomic-datefirst-var", "query-atomic-options"),
        ("atomic-language-var", "query-atomic-options"),
        ("atomic-durability-bad", "query-atomic-options"),
        ("atomic-language-no-equals", "query-atomic-options"),
    ] {
        let sql = source(id);
        let mut p = Parser::new();
        p.set_language(&tree_sitter_mssql::LANGUAGE.into()).unwrap();
        assert!(
            !p.parse(sql, None).unwrap().root_node().has_error(),
            "{id}: this boundary is enforced by constraints"
        );
        assert_eq!(rules(sql), [expected], "{id}");
    }
    for id in [
        "offsets-valid",
        "offsets-repeat",
        "cursor-iso-reverse",
        "cursor-iso-repeat",
        "cursor-set-repeat",
        "cursor-duplicate-local",
        "cursor-global-comment",
        "cursor-comment-literal",
        "exec-repeat-comment",
        "predict-schema",
        "predict-collate",
        "atomic-all",
        "atomic-datefirst-eight",
        "atomic-format-bad",
        "atomic-language-literal-keyword",
        "parse-using",
        "try-parse",
        "update-variable-chain",
        "update-compound-chain",
        "insert-delete-output",
        "insert-update-output",
        "insert-insert-output",
        "procedure-native",
    ] {
        assert!(rules(source(id)).is_empty(), "{id}");
    }
}

#[test]
fn backup_restore_options_match_the_engine() {
    for ok in [
        "BACKUP DATABASE d FILE = 'f', FILEGROUP = @g, READ_WRITE_FILEGROUPS TO DISK = 'a', @dev MIRROR TO URL = N'u' WITH FORMAT, STATS, STATS = -1, NAME = N'n', COMPRESSION (ALGORITHM = ZSTD, LEVEL = HIGH), ENCRYPTION (ALGORITHM = AES_256, SERVER CERTIFICATE = c);",
        "BACKUP GROUP a, [b] TO DISK = 'a' WITH METADATA_ONLY;",
        "RESTORE LOG d PAGE = '1:57' FROM dev, DATABASE_SNAPSHOT = 's' WITH FILE = 2, MOVE N'a' TO @b, STOPATMARK = 'm' AFTER @t, FILESTREAM (DIRECTORY_NAME = 'x'), RECOVERY, NORECOVERY;",
        "RESTORE HEADERONLY FROM VIRTUAL_DEVICE = 'v' WITH PARTIAL, FILE = @f;",
        "BACKUP CERTIFICATE c TO FILE = 'a' WITH PRIVATE KEY (ENCRYPTION BY PASSWORD = 'p'), FORMAT = 'PFX', PRIVATE KEY (FILE = 'b', ALGORITHM = 'AES_256');",
        "BACKUP MASTER KEY TO URL = 'u' ENCRYPTION BY PASSWORD = 'p';",
    ] {
        assert!(rules(ok).is_empty(), "{ok}");
    }
    for (bad, rule) in [
        (
            "BACKUP DATABASE d FOO = 'f' TO DISK = 'a';",
            "backup-restore-file",
        ),
        (
            "BACKUP DATABASE d FILE = 1 TO DISK = 'a';",
            "backup-restore-file",
        ),
        (
            "RESTORE LOG d READ_WRITE_FILEGROUPS FROM DISK = 'a';",
            "backup-restore-file",
        ),
        ("BACKUP DATABASE d TO PIPE = 'a';", "backup-restore-device"),
        ("BACKUP DATABASE d TO DISK 'a';", "backup-restore-device"),
        (
            "RESTORE DATABASE d FROM DISK = foo;",
            "backup-restore-device",
        ),
        ("BACKUP GROUP @a TO DISK = 'a';", "backup-restore-device"),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH FOO;",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH REPLACE;",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH INIT = 1;",
            "backup-restore-option",
        ),
        (
            "RESTORE DATABASE d FROM DISK = 'a' WITH CREDENTIAL;",
            "backup-restore-option",
        ),
        (
            "RESTORE DATABASE d FROM DISK = 'a' WITH FILE = 'x';",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH NAME = foo;",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH BLOCKSIZE = 0x10;",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH MOVE 'a' TO 'b';",
            "backup-restore-option",
        ),
        (
            "RESTORE DATABASE d FROM DISK = 'a' WITH MOVE 1 TO 2;",
            "backup-restore-option",
        ),
        (
            "RESTORE LOG d FROM DISK = 'a' WITH STOPAT = 'x' AFTER 'y';",
            "backup-restore-option",
        ),
        (
            "RESTORE LOG d FROM DISK = 'a' WITH STOPATMARK = 'm' AFTER 1;",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH COMPRESSION (LEVEL = LOW, ALGORITHM = ZSTD);",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH ENCRYPTION (ALGORITHM = FOO, SERVER CERTIFICATE = c);",
            "backup-restore-option",
        ),
        (
            "BACKUP DATABASE d TO DISK = 'a' WITH ENCRYPTION (ALGORITHM = AES_256, SERVER CERTIFICATE = @c);",
            "backup-restore-option",
        ),
        (
            "RESTORE DATABASE d FROM DISK = 'a' WITH FILESTREAM (DIRECTORY_NAME = @v);",
            "backup-restore-option",
        ),
        (
            "RESTORE DATABASE d FROM DISK = 'a' WITH COMPRESSION;",
            "backup-restore-option",
        ),
        (
            "BACKUP CERTIFICATE c TO FILE = 'a' WITH FORMAT = 'PFX', FORMAT = 'PFX';",
            "backup-certificate-options",
        ),
        (
            "BACKUP CERTIFICATE c TO FILE = 'a' WITH PRIVATE KEY (ENCRYPTION BY PASSWORD = 'p');",
            "backup-certificate-options",
        ),
        (
            "BACKUP CERTIFICATE c TO FILE = 'a' WITH PRIVATE KEY (FILE = 'b', ENCRYPTION BY PASSWORD = @p);",
            "backup-certificate-options",
        ),
        (
            "BACKUP CERTIFICATE c TO FILE = 'a' WITH FOO = 'x';",
            "backup-certificate-options",
        ),
    ] {
        assert_eq!(rules(bad), [rule], "{bad}");
    }
}
#[test]
fn ddl_rest_misc_matches_engine_neighbours() {
    for bad in [
        "COMMIT WITH (WHATEVER = ON);",
        "COMMIT WITH (DELAYED_DURABILITY = ON, DELAYED_DURABILITY = OFF);",
        "COMMIT WITH (DELAYED_DURABILITY = 1);",
        "COMMIT WITH (DELAYED_DURABILITY);",
        "KILL QUERY NOTIFICATION SUBSCRIPTION 'x';",
        "UPDATE STATISTICS dbo.t WITH FULLSCAN ON PARTITIONS (1);",
        "UPDATE STATISTICS dbo.t WITH FULLSCAN, COLUMNS, INDEX;",
        "UPDATE STATISTICS dbo.t WITH WHATEVER = 1;",
        "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (RANK = WRONG);",
        "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (LABEL = 1);",
        "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (LABEL = 'a', LABEL = 'b');",
        "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (WHATEVER = 'a');",
        "DROP SENSITIVITY CLASSIFICATION FROM c;",
        "ADD SIGNATURE TO dbo.p BY CERTIFICATE c WITH WHATEVER = 1;",
        "ADD SIGNATURE TO dbo.p BY SYMMETRIC KEY k WITH PASSWORD = 'x';",
    ] {
        assert!(!parse(bad).root_node().has_error(), "{bad}");
        assert_eq!(rules(bad), ["ddl-rest-misc"], "{bad}");
    }
    for good in [
        "COMMIT;",
        "COMMIT TRAN t WITH (DELAYED_DURABILITY = ON);",
        "COMMIT WORK WITH (DELAYED_DURABILITY = OFF);",
        "KILL QUERY NOTIFICATION SUBSCRIPTION ALL;",
        "KILL STATS JOB -1;",
        "UPDATE STATISTICS dbo.t WITH RESAMPLE ON PARTITIONS (1, 3 TO 5), COLUMNS;",
        "ADD SENSITIVITY CLASSIFICATION TO dbo.t.c WITH (LABEL = N'기밀', RANK = HIGH);",
        "DROP SENSITIVITY CLASSIFICATION FROM dbo.t.c;",
        "ADD SIGNATURE TO dbo.p BY CERTIFICATE c WITH SIGNATURE = 0x0102;",
        "ADD SIGNATURE TO dbo.p BY SYMMETRIC KEY k;",
    ] {
        assert!(!parse(good).root_node().has_error(), "{good}");
        assert!(rules(good).is_empty(), "{good}");
    }
}
#[test]
fn ddl_rest_closing_neighbours_match_the_engine() {
    for (sql, rule) in [
        (
            "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE (sqlserver.database_id) < 8);",
            "ddl-rest-xe-source",
        ),
        (
            "ALTER EVENT SESSION es ON SERVER ADD EVENT sqlserver.sql_statement_completed (WHERE (duration) < 8);",
            "ddl-rest-xe-source",
        ),
        (
            "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE sqlserver.equal_i_sql_unicode_string((sqlserver.client_app_name), N'x'));",
            "ddl-rest-xe-source",
        ),
        ("DROP SEQUENCE db1.dbo.seq_a;", "ddl-rest-drop-sequence"),
        ("DROP SEQUENCE db1..seq_a;", "ddl-rest-drop-sequence"),
        ("DROP SEQUENCE srv.db1.dbo.seq_a;", "ddl-rest-drop-sequence"),
        (
            "DROP SEQUENCE dbo.seq_a, db1.dbo.seq_b;",
            "ddl-rest-drop-sequence",
        ),
        (
            "DROP SEQUENCE IF EXISTS db1.dbo.seq_a;",
            "ddl-rest-drop-sequence",
        ),
        (
            "REVOKE CONNECT ON AVAILABILITY GROUP::ag FROM u;",
            "ddl-rest-ag-connect",
        ),
        (
            "REVOKE CONNECT ON AVAILABILITY GROUP::ag TO u;",
            "ddl-rest-ag-connect",
        ),
        (
            "GRANT CONNECT ON AVAILABILITY GROUP::ag TO u;",
            "ddl-rest-ag-connect",
        ),
        (
            "DENY CONNECT ON AVAILABILITY GROUP::ag TO u;",
            "ddl-rest-ag-connect",
        ),
        (
            "GRANT CONTROL ON [ASYMMETRIC KEY]::ak TO u;",
            "ddl-rest-securable-class",
        ),
        (
            r"REVOKE ALTER ON [AVAILABILITY GROUP]::[ag_sales] TO [CONTOSO\Mary];",
            "ddl-rest-securable-class",
        ),
    ] {
        let tree = parse(sql);
        assert!(
            !tree.root_node().has_error(),
            "{sql}: this boundary is enforced by constraints"
        );
        assert_eq!(rules(sql), [rule], "{sql}");
    }
    for sql in [
        "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE sqlserver.database_id < 8);",
        "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE ((sqlserver.database_id < 8)));",
        "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE (sqlserver.database_id < 8) AND NOT (duration > 5));",
        "ALTER EVENT SESSION es ON DATABASE ADD EVENT sqlserver.sql_statement_completed (WHERE sqlserver.equal_i_sql_unicode_string(sqlserver.client_app_name, N'x'));",
        "DROP SEQUENCE dbo.seq_a, seq_b;",
        "DROP SEQUENCE IF EXISTS dbo.seq_a;",
        "REVOKE ALTER ON AVAILABILITY GROUP::ag TO u;",
        "REVOKE CONTROL ON AVAILABILITY GROUP::ag FROM u;",
        "GRANT ALTER ON AVAILABILITY GROUP::ag TO u;",
        "DENY CONTROL ON AVAILABILITY GROUP::ag TO u;",
        r"REVOKE GRANT OPTION FOR CONTROL ON AVAILABILITY GROUP::ag_sales FROM l_app, [CONTOSO\Mary], l_cert, l_akey CASCADE AS l_admin;",
        r"REVOKE ALTER ON AVAILABILITY/* 한국어 C:\audit */GROUP::[ag_sales] TO [CONTOSO\Mary];",
    ] {
        let tree = parse(sql);
        assert!(!tree.root_node().has_error(), "{sql}");
        assert!(rules(sql).is_empty(), "{sql}");
    }
}

#[test]
fn deep_checks_have_linear_node_visit_growth() {
    for class in ["and", "case", "while", "function"] {
        let mut previous = 0;
        let mut first_time = std::time::Duration::ZERO;
        for n in [256, 512, 1024] {
            let sql = match class {
                "and" => format!("SELECT 1 WHERE {};", vec!["a = 1"; n].join(" AND ")),
                "case" => format!(
                    "SELECT {}0{};",
                    "CASE WHEN a = 1 THEN 1 ELSE ".repeat(n),
                    " END".repeat(n)
                ),
                "while" => format!(
                    "{}SET @a = 0;{}",
                    "WHILE @a > 0 BEGIN ".repeat(n),
                    " END".repeat(n)
                ),
                _ => format!("SELECT {}a{};", "ISNULL(".repeat(n), ", 0)".repeat(n)),
            };
            let tree = parse(&sql);
            assert!(!tree.root_node().has_error(), "{class}/{n}");
            crate::tree::take_visits();
            assert!(check(&tree, sql.as_bytes()).is_empty(), "{class}/{n}");
            let visits = crate::tree::take_visits();
            assert!(visits > n, "the guard must exercise the traversal");
            assert!(
                previous == 0 || visits * 10 < previous * 23,
                "{class}/{n}: {previous} -> {visits} visits"
            );
            previous = visits;
            // Minima and a noise floor tolerate CI pauses; catch uncounted FFI parent searches too.
            let elapsed = (0..5)
                .map(|_| {
                    let start = std::time::Instant::now();
                    assert!(check(&tree, sql.as_bytes()).is_empty());
                    start.elapsed()
                })
                .min()
                .unwrap();
            if n == 256 {
                first_time = elapsed;
            } else if n == 1024 {
                assert!(
                    elapsed < (first_time * 8).max(std::time::Duration::from_millis(20)),
                    "{class}: 4x input took {first_time:?} -> {elapsed:?}"
                );
            }
        }
    }
}

#[test]
fn preorder_context_matches_tree_ancestry() {
    let tree = parse(
        "WHILE @a > 0 BEGIN SELECT ISNULL((SELECT CASE WHEN a = 1 THEN 1 ELSE 0 END), 0); END;",
    );
    let mut context = util::Context::default();
    for (node, depth) in crate::tree::preorder_with_depth(tree.root_node()) {
        context.enter(node, depth);
        assert_eq!(context.parent(), node.parent());
        assert_eq!(
            context.grandparent(),
            node.parent().and_then(|n| n.parent())
        );
        for kind in [
            "invocation",
            "query_specification",
            "while_statement",
            "literal",
            node.kind(),
        ] {
            let expected = crate::tree::ancestors(node).find(|n| n.kind() == kind);
            assert_eq!(context.ancestor(kind), expected);
            assert_eq!(
                context.ancestor_parent(kind),
                expected.and_then(|n| n.parent())
            );
        }
    }
}
