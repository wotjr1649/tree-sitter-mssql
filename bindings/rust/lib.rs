//! This crate provides T-SQL (Microsoft SQL Server) language support for the [tree-sitter][] parsing library.
//!
//! Typically, you will use the [LANGUAGE][] constant to add this language to a
//! tree-sitter [Parser][], and then use the parser to parse some code:
//!
//! ```
//! let code = r#"
//! SELECT TOP (10) name FROM sys.objects WHERE type = 'U';
//! "#;
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_mssql::LANGUAGE;
//! parser
//!     .set_language(&language.into())
//!     .expect("Error loading T-SQL parser");
//! let tree = parser.parse(code, None).unwrap();
//! assert!(!tree.root_node().has_error());
//! ```
//!
//! [Parser]: https://docs.rs/tree-sitter/*/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_mssql() -> *const ();
}

/// The tree-sitter [`LanguageFn`][LanguageFn] for this grammar.
///
/// [LanguageFn]: https://docs.rs/tree-sitter-language/*/tree_sitter_language/struct.LanguageFn.html
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_mssql) };

/// The content of the [`node-types.json`][] file for this grammar.
///
/// [`node-types.json`]: https://tree-sitter.github.io/tree-sitter/using-parsers#static-node-types
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");
pub const INDENTS_QUERY: &str = include_str!("../../queries/indents.scm");
pub const INJECTIONS_QUERY: &str = include_str!("../../queries/injections.scm");
pub const LOCALS_QUERY: &str = include_str!("../../queries/locals.scm");
pub const TAGS_QUERY: &str = include_str!("../../queries/tags.scm");

#[cfg(test)]
mod tests {
    use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

    fn incremental_lexes(mut source: String, at: usize, replacement: &str) -> usize {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use tree_sitter::{InputEdit, Point};

        let mut parser = Parser::new();
        parser.set_language(&super::LANGUAGE.into()).unwrap();
        let mut old = parser.parse(&source, None).unwrap();
        assert!(!old.root_node().has_error());
        let prefix = &source[..at];
        let row = prefix.bytes().filter(|&c| c == b'\n').count();
        let column = prefix.rsplit('\n').next().unwrap().len();
        assert_eq!(replacement.len(), 1);
        source.replace_range(at..at + 1, replacement);
        old.edit(&InputEdit {
            start_byte: at,
            old_end_byte: at + 1,
            new_end_byte: at + 1,
            start_position: Point::new(row, column),
            old_end_position: Point::new(row, column + 1),
            new_end_position: Point::new(row, column + 1),
        });
        let lexed = Arc::new(AtomicUsize::new(0));
        let logger_count = Arc::clone(&lexed);
        // The public logger counts newly lexed tokens independently of wall-clock time.
        parser.set_logger(Some(Box::new(move |_, message| {
            if message.starts_with("lexed_lookahead ") {
                logger_count.fetch_add(1, Ordering::Relaxed);
            }
        })));
        let incremental = parser.parse(&source, Some(&old)).unwrap();
        parser.set_logger(None);
        let fresh = parser.parse(&source, None).unwrap();
        assert!(!incremental.root_node().has_error());
        assert_eq!(
            incremental.root_node().to_sexp(),
            fresh.root_node().to_sexp()
        );
        let mut a = incremental.walk();
        let mut b = fresh.walk();
        loop {
            assert_eq!(a.node().kind_id(), b.node().kind_id());
            assert_eq!(a.node().range(), b.node().range());
            assert_eq!(a.node().is_missing(), b.node().is_missing());
            assert_eq!(a.node().is_extra(), b.node().is_extra());
            assert_eq!(a.field_name(), b.field_name());
            let down = a.goto_first_child();
            assert_eq!(down, b.goto_first_child());
            if down {
                continue;
            }
            loop {
                let next = a.goto_next_sibling();
                assert_eq!(next, b.goto_next_sibling());
                if next {
                    break;
                }
                let up = a.goto_parent();
                assert_eq!(up, b.goto_parent());
                if !up {
                    break;
                }
            }
            if a.node() == incremental.root_node() {
                break;
            }
        }
        let n = lexed.load(Ordering::Relaxed);
        assert!(n > 0);
        n
    }

    #[test]
    fn incremental_insert_relexing_stays_bounded() {
        for (semicolon, procedure) in [(false, false), (true, false), (false, true)] {
            let mut counts = Vec::new();
            for count in [256, 1024] {
                let prefix = if procedure {
                    "CREATE PROCEDURE dbo.p AS BEGIN\n"
                } else {
                    ""
                };
                let statement = if semicolon {
                    "INSERT INTO dbo.t (a) VALUES (1);\n"
                } else {
                    "INSERT INTO dbo.t (a) VALUES (1)\n"
                };
                let source = format!(
                    "{prefix}{}{}",
                    statement.repeat(count),
                    if procedure { "END\n" } else { "" }
                );
                let at =
                    prefix.len() + count / 2 * statement.len() + statement.find("dbo").unwrap();
                let n = incremental_lexes(source, at, "x");
                eprintln!(
                    "semicolon={semicolon} procedure={procedure} statements={count} lexed={n}"
                );
                counts.push(n);
            }
            assert!(
                counts.iter().all(|&n| n <= 32),
                "relexed tokens: {counts:?}"
            );
            assert!(
                counts[1] <= counts[0] + 2,
                "relexing grows with input: {counts:?}"
            );
        }
    }

    #[test]
    fn incremental_create_table_name_relexing_stays_bounded() {
        let mut counts = Vec::new();
        for count in [256, 1024] {
            let columns = (0..count)
                .map(|i| format!("c{i} int"))
                .collect::<Vec<_>>()
                .join(", ");
            let source = format!("CREATE TABLE dbo.t ({columns});\n");
            let n = incremental_lexes(source, "CREATE TABLE ".len(), "x");
            eprintln!("table columns={count} lexed={n}");
            counts.push(n);
        }
        assert!(counts.iter().all(|&n| n <= 4), "relexed tokens: {counts:?}");
        assert!(
            counts[1] <= counts[0] + 2,
            "relexing grows with input: {counts:?}"
        );
    }

    #[test]
    fn incremental_procedure_header_relexing_stays_bounded() {
        let mut counts = Vec::new();
        for count in [256, 1024] {
            let source = format!(
                "CREATE PROCEDURE dbo.p AS BEGIN\n{}END\nGO\n",
                "UPDATE dbo.t SET a = 1;\n".repeat(count)
            );
            let n = incremental_lexes(source, "CREATE PROCEDURE dbo.".len(), "x");
            eprintln!("procedure UPDATE statements={count} lexed={n}");
            counts.push(n);
        }
        assert!(
            counts.iter().all(|&n| n <= 8),
            "procedure header relexes too much: {counts:?}"
        );
        assert!(
            counts[1] <= counts[0] + 2,
            "relexing grows with input: {counts:?}"
        );
    }

    #[test]
    fn incremental_merge_joined_source_relexing_stays_bounded() {
        let source = r#"CREATE TABLE Person
(
    ID INTEGER PRIMARY KEY,
    PersonName VARCHAR(100)
)
AS NODE
GO
CREATE TABLE City
(
    ID INTEGER PRIMARY KEY,
    CityName VARCHAR(100),
    StateName VARCHAR(100)
)
AS NODE
GO
CREATE TABLE livesIn
(
    StreetAddress VARCHAR(100)
)
AS EDGE
GO
INSERT INTO Person VALUES (1, 'Ron'), (2, 'David'), (3, 'Nancy')
GO
INSERT INTO City VALUES (1, 'Redmond', 'Washington'), (2, 'Seattle', 'Washington')
GO
INSERT livesIn SELECT P.$node_id, C.$node_id, c
FROM Person P, City C, (values (1,1, '123 Avenue'), (2,2,'Main Street')) v(a,b,c)
WHERE P.id = a AND C.id = b
GO
CREATE OR ALTER PROCEDURE mergeEdge
    @PersonId integer,
    @CityId integer,
    @StreetAddress varchar(100)
AS
BEGIN
    MERGE livesIn
        USING ((SELECT @PersonId, @CityId, @StreetAddress) AS T (PersonId, CityId, StreetAddress)
                JOIN Person ON T.PersonId = Person.ID
                JOIN City ON T.CityId = City.ID)
        ON MATCH (Person-(livesIn)->City)
    WHEN MATCHED THEN
        UPDATE SET StreetAddress = @StreetAddress
    WHEN NOT MATCHED THEN
        INSERT ($from_id, $to_id, StreetAddress)
        VALUES (Person.$node_id, City.$node_id, @StreetAddress) ;
END
GO
EXEC mergeEdge 3, 2, '4444th Avenue'
GO
EXEC mergeEdge 1, 1, '321 Avenue'
GO
SELECT PersonName, CityName, StreetAddress
FROM Person , City , livesIn
WHERE MATCH(Person-(livesIn)->city)
GO
"#
        .to_string();
        let at = source.find("MERGE livesIn").unwrap();
        let n = incremental_lexes(source, at, "m");
        assert!(n <= 11, "MERGE joined source relexes too much: {n}");
    }

    #[test]
    fn test_can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading T-SQL parser");
    }

    /// The text of every `capture` node the query finds in `source`, in
    /// document order. Duplicates are kept, so a query that captures one
    /// node twice fails the test that expects it once.
    fn captures(query_source: &str, source: &str, capture: &str) -> Vec<String> {
        let language = super::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "sample does not parse: {source}"
        );
        let query = Query::new(&language, query_source).unwrap();
        let index = query.capture_index_for_name(capture).unwrap();
        let mut cursor = QueryCursor::new();
        let mut found = Vec::new();
        let mut it = cursor.captures(&query, tree.root_node(), source.as_bytes());
        while let Some((m, i)) = it.next() {
            let c = m.captures()[*i];
            if c.index == index {
                found.push((
                    c.node.start_byte(),
                    c.node.utf8_text(source.as_bytes()).unwrap().to_string(),
                ));
            }
        }
        found.sort();
        found.into_iter().map(|(_, text)| text).collect()
    }

    #[test]
    fn tags_name_each_definition_but_not_a_trigger_table() {
        let source = "\
CREATE PROCEDURE dbo.load_orders AS BEGIN RETURN END
GO
CREATE FUNCTION dbo.order_total (@id int) RETURNS int AS BEGIN RETURN 1 END
GO
CREATE TRIGGER dbo.trg_orders ON dbo.orders AFTER INSERT AS SELECT 1;
GO
CREATE TABLE dbo.orders (id int);
CREATE VIEW dbo.open_orders AS SELECT id FROM dbo.orders;
CREATE TYPE dbo.order_id FROM int;
CREATE INDEX ix_orders_id ON dbo.orders (id);
";
        assert_eq!(
            captures(super::TAGS_QUERY, source, "name"),
            [
                "dbo.load_orders",
                "dbo.order_total",
                "dbo.trg_orders",
                "dbo.orders",
                "dbo.open_orders",
                "dbo.order_id",
                "ix_orders_id",
            ]
        );
    }

    #[test]
    fn locals_define_every_alias_and_cte_name() {
        let source = "\
WITH c (x) AS (SELECT 1 AS x)
SELECT * FROM t AS a
JOIN OPENJSON(@j) WITH (v int) AS oj ON 1 = 1
CROSS JOIN OPENJSON(@k) WITH (w int) ok
CROSS APPLY (VALUES (1)) AS ap (n)
PIVOT (MAX(v) FOR w IN ([1])) AS pv;
SELECT f.v FROM OPENJSON(@j) WITH (v int) AS f;
MERGE dbo.t AS tgt USING dbo.s AS src ON tgt.id = src.id
WHEN MATCHED THEN DELETE;
";
        assert_eq!(
            captures(super::LOCALS_QUERY, source, "local.definition"),
            ["c", "a", "oj", "ok", "ap", "pv", "f", "tgt", "src"]
        );
    }

    #[test]
    fn injections_cover_exec_literals_and_their_concatenation() {
        let source = "\
EXEC('SELECT 1');
EXEC('SELECT * FROM ' + @t + ' WHERE a = ' + @a + ' AND b = 1' + @b + '' + @c + ' ORDER BY 1');
EXECUTE(N'SELECT 2') AS USER = 'u';
EXEC sp_executesql N'SELECT @p', N'@p int', @p = 1;
SELECT 'not' + 'sql';
EXEC('SELECT TOP ' + 10 + ' x');
EXEC('a' - 'b');
EXEC(10);
";
        assert_eq!(
            captures(super::INJECTIONS_QUERY, source, "injection.content"),
            [
                "'SELECT 1'",
                "'SELECT * FROM '",
                "' WHERE a = '",
                "' AND b = 1'",
                "''",
                "' ORDER BY 1'",
                "N'SELECT 2'",
                "N'SELECT @p'",
                "'SELECT TOP '",
                "' x'",
            ]
        );
    }
}
