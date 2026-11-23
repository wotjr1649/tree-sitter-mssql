use super::Violation;
use crate::tree::{ancestors, has_child, named_children, preorder};
use tree_sitter::{Node, TreeCursor};

/// Constraints inspect syntax; comments are named extras, never positional children or siblings.
pub(super) trait SyntaxNode<'tree> {
    fn syntax_children<'cursor>(
        &self,
        cursor: &'cursor mut TreeCursor<'tree>,
    ) -> impl Iterator<Item = Node<'tree>> + 'cursor;
    fn syntax_named_children<'cursor>(
        &self,
        cursor: &'cursor mut TreeCursor<'tree>,
    ) -> impl Iterator<Item = Node<'tree>> + 'cursor;
    fn syntax_child(&self, index: usize) -> Option<Node<'tree>>;
    fn syntax_named_child(&self, index: usize) -> Option<Node<'tree>>;
    fn syntax_named_child_count(&self) -> usize;
    fn syntax_next_sibling(&self) -> Option<Node<'tree>>;
    fn syntax_prev_sibling(&self) -> Option<Node<'tree>>;
    fn syntax_next_named_sibling(&self) -> Option<Node<'tree>>;
    fn syntax_prev_named_sibling(&self) -> Option<Node<'tree>>;
}

impl<'tree> SyntaxNode<'tree> for Node<'tree> {
    fn syntax_children<'cursor>(
        &self,
        cursor: &'cursor mut TreeCursor<'tree>,
    ) -> impl Iterator<Item = Node<'tree>> + 'cursor {
        self.children(cursor).filter(|node| !node.is_extra())
    }

    fn syntax_named_children<'cursor>(
        &self,
        cursor: &'cursor mut TreeCursor<'tree>,
    ) -> impl Iterator<Item = Node<'tree>> + 'cursor {
        self.syntax_children(cursor).filter(|node| node.is_named())
    }

    fn syntax_child(&self, index: usize) -> Option<Node<'tree>> {
        self.syntax_children(&mut self.walk()).nth(index)
    }

    fn syntax_named_child(&self, index: usize) -> Option<Node<'tree>> {
        self.syntax_named_children(&mut self.walk()).nth(index)
    }

    fn syntax_named_child_count(&self) -> usize {
        self.syntax_named_children(&mut self.walk()).count()
    }

    fn syntax_next_sibling(&self) -> Option<Node<'tree>> {
        std::iter::successors(self.next_sibling(), Node::next_sibling).find(|node| !node.is_extra())
    }

    fn syntax_prev_sibling(&self) -> Option<Node<'tree>> {
        std::iter::successors(self.prev_sibling(), Node::prev_sibling).find(|node| !node.is_extra())
    }

    fn syntax_next_named_sibling(&self) -> Option<Node<'tree>> {
        std::iter::successors(self.syntax_next_sibling(), SyntaxNode::syntax_next_sibling)
            .find(Node::is_named)
    }

    fn syntax_prev_named_sibling(&self) -> Option<Node<'tree>> {
        std::iter::successors(self.syntax_prev_sibling(), SyntaxNode::syntax_prev_sibling)
            .find(Node::is_named)
    }
}

pub(super) fn string_literal(node: Node, source: &[u8]) -> bool {
    let text = &source[node.byte_range()];
    node.kind() == "literal"
        && (text.starts_with(b"'") || text.starts_with(b"N'") || text.starts_with(b"n'"))
}

pub(super) fn empty_string(node: Node, source: &[u8]) -> bool {
    matches!(
        &source[node.byte_range()],
        b"''" | b"N''" | b"n''" | b"\"\""
    )
}

pub(super) fn integer_literal(node: Node, source: &[u8]) -> bool {
    let text = &source[node.byte_range()];
    node.kind() == "literal"
        && (!text.is_empty() && text.iter().all(u8::is_ascii_digit)
            || text
                .strip_prefix(b"0x")
                .or_else(|| text.strip_prefix(b"0X"))
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit)))
}

pub(super) fn local_variable(node: Node, source: &[u8]) -> bool {
    let text = &source[node.byte_range()];
    matches!(node.kind(), "field" | "identifier")
        && text.starts_with(b"@")
        && !text.starts_with(b"@@")
}

// QUOTED_IDENTIFIER is explicit in the oracle's quoted-language neighbours.
pub(super) fn quoted_strings(node: Node, source: &[u8]) -> bool {
    let root = ancestors(node).last().unwrap_or(node);
    let mut strings = false;
    for n in preorder(root) {
        if n.start_byte() >= node.start_byte() {
            break;
        }
        if n.kind() == "set_statement" {
            let children: Vec<_> = named_children(n);
            if children.iter().any(|p| {
                p.kind() == "object_reference" && upper_text(*p, source) == "QUOTED_IDENTIFIER"
            }) {
                if children.iter().any(|p| p.kind() == "keyword_off") {
                    strings = true;
                }
                if children.iter().any(|p| p.kind() == "keyword_on") {
                    strings = false;
                }
            }
        }
    }
    strings
}

/// A name without one pair of brackets or double quotes: `[INT]` and `"INT"` are `INT`.
pub(super) fn undelimited(text: &str) -> &str {
    text.strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .or_else(|| text.strip_prefix('"').and_then(|t| t.strip_suffix('"')))
        .unwrap_or(text)
}

/// The upper-case base name of a type node, None for a qualified or parameterized-name type.
pub(super) fn base_type(ty: Node, source: &[u8]) -> Option<String> {
    let name = match ty.kind() {
        "custom_type" => {
            let reference = ty.syntax_named_child(0)?;
            if reference.syntax_named_child_count() != 1 {
                return None;
            }
            undelimited(&upper_text(reference, source)).to_string()
        }
        "double" if has_child(ty, "keyword_real") => "REAL".into(),
        "double" => "FLOAT".into(),
        kind => kind.trim_start_matches("keyword_").to_ascii_uppercase(),
    };
    Some(match name.as_str() {
        "INTEGER" => "INT".into(),
        "DEC" => "DECIMAL".into(),
        _ => name,
    })
}

pub(super) fn upper_text(node: Node, source: &[u8]) -> String {
    if node.child_count() == 0 && !node.is_extra() {
        return node
            .utf8_text(source)
            .unwrap_or_default()
            .to_ascii_uppercase();
    }
    let mut text = String::new();
    let mut at = node.start_byte();
    for part in preorder(node).filter(Node::is_extra) {
        // An extra replaces its entire subtree, including nested extras.
        if part != node
            && std::iter::once(node)
                .chain(ancestors(part).take_while(|p| *p != node))
                .any(|p| p.is_extra())
        {
            continue;
        }
        text.push_str(std::str::from_utf8(&source[at..part.start_byte()]).unwrap_or_default());
        at = part.end_byte();
        text.push(' ');
    }
    text.push_str(std::str::from_utf8(&source[at..node.end_byte()]).unwrap_or_default());
    text.to_ascii_uppercase()
}

pub(super) fn violation(rule: &'static str, detail: String, node: Node) -> Violation {
    let at = node.start_position();
    Violation {
        rule,
        detail,
        row: at.row,
        column: at.column,
    }
}

/// Only direct options count: a nested option cannot satisfy its container's requirements.
/// BUFFER POOL evidence: ddl-database/server/negative-suspected-001/002, Msg 102 at levels 100-170
/// on SQL Server 2025 LocalDB 17.0.1000.7; server/positive-011 accepts both FILENAME and SIZE.
pub(super) fn option_set(
    container: Node,
    source: &[u8],
    required_rule: &'static str,
    duplicate_rule: &'static str,
    required: &[&str],
    unique: &[&str],
    out: &mut Vec<Violation>,
) {
    let mut seen = std::collections::BTreeSet::new();
    for option in named_children(container)
        .into_iter()
        .filter(|n| n.kind() == "option")
    {
        let Some(name) = option.child_by_field_name("name") else {
            continue;
        };
        let key = upper_text(name, source);
        let duplicate = !seen.insert(key.clone());
        if unique.contains(&key.as_str()) && duplicate {
            out.push(violation(
                duplicate_rule,
                format!("{key} appears more than once in {}", container.kind()),
                option,
            ));
        }
    }
    for key in required {
        if !seen.iter().any(|name| name.as_str() == *key) {
            out.push(violation(
                required_rule,
                format!("{} needs {key}", container.kind()),
                container,
            ));
        }
    }
}

pub(super) fn direct_options(node: Node) -> Vec<Node> {
    named_children(node)
        .into_iter()
        .filter(|n| n.kind() == "option")
        .collect()
}

pub(super) fn option_name(option: Node, source: &[u8]) -> String {
    option
        .child_by_field_name("name")
        .map(|n| upper_text(n, source))
        .unwrap_or_default()
}

pub(super) fn ancestor<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    ancestors(node).find(|n| n.kind() == kind)
}

/// The current preorder path: parent lookup never searches from the tree root.
#[derive(Default)]
pub(super) struct Context<'tree> {
    path: Vec<Node<'tree>>,
    by_kind: std::collections::HashMap<&'tree str, Vec<usize>>,
}

impl<'tree> Context<'tree> {
    pub fn enter(&mut self, node: Node<'tree>, depth: usize) {
        #[cfg(test)]
        crate::tree::record_visit();
        while self.path.len() > depth {
            #[cfg(test)]
            crate::tree::record_visit();
            let previous = self.path.pop().unwrap();
            self.by_kind.get_mut(previous.kind()).unwrap().pop();
        }
        self.path.push(node);
        self.by_kind.entry(node.kind()).or_default().push(depth);
    }

    pub fn parent(&self) -> Option<Node<'tree>> {
        #[cfg(test)]
        crate::tree::record_visit();
        self.path.iter().rev().nth(1).copied()
    }

    pub fn grandparent(&self) -> Option<Node<'tree>> {
        self.path.iter().rev().nth(2).copied()
    }

    pub fn ancestor(&self, kind: &str) -> Option<Node<'tree>> {
        self.ancestor_index(kind).map(|i| self.path[i])
    }

    pub fn ancestor_parent(&self, kind: &str) -> Option<Node<'tree>> {
        self.ancestor_index(kind)?
            .checked_sub(1)
            .map(|i| self.path[i])
    }

    fn ancestor_index(&self, kind: &str) -> Option<usize> {
        #[cfg(test)]
        crate::tree::record_visit();
        self.by_kind
            .get(kind)?
            .iter()
            .rev()
            .find(|&&i| i + 1 < self.path.len())
            .copied()
    }
}
