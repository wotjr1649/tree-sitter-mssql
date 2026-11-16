//! Tree traversal and byte positions shared by the checker and CLI.

use tree_sitter::{Node, Point};

/// Visit a subtree in document order, including its root and extras.
pub fn preorder(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    let mut cursor = node.walk();
    std::iter::successors(Some(node), move |_| {
        if !cursor.goto_first_child() {
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    return None;
                }
            }
        }
        Some(cursor.node())
    })
}

pub fn ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    std::iter::successors(node.parent(), Node::parent)
}

pub fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    node.named_children(&mut node.walk())
        .filter(|child| !child.is_extra())
        .collect()
}

pub fn has_child(node: Node<'_>, kind: &str) -> bool {
    node.children(&mut node.walk())
        .any(|child| !child.is_extra() && child.kind() == kind)
}

/// Row and byte column of a byte offset, as tree-sitter counts them.
pub(crate) fn point(source: &[u8], byte: usize) -> Point {
    let before = &source[..byte];
    let row = before.iter().filter(|&&b| b == b'\n').count();
    let line_start = before
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    Point {
        row,
        column: byte - line_start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_keeps_subtrees_extras_and_document_order() {
        let mut parser = crate::grammar_parser().unwrap();
        let tree = crate::parse(&mut parser, b"SELECT /*comment*/ 1; SELECT 2;").unwrap();
        let root = tree.root_node();
        let all: Vec<_> = preorder(root).collect();
        assert_eq!(all.first(), Some(&root));
        assert!(all.iter().any(Node::is_extra));
        let first = *all.iter().find(|n| n.kind() == "select").unwrap();
        let subtree: Vec<_> = preorder(first).collect();
        assert_eq!(subtree.first(), Some(&first));
        assert!(subtree.len() > 1);
        assert!(subtree.iter().all(|n| n.end_byte() <= first.end_byte()));
        assert!(
            subtree
                .windows(2)
                .all(|pair| pair[0].start_byte() <= pair[1].start_byte())
        );
        assert_eq!(ancestors(first).next(), first.parent());
        assert!(first.end_byte() < root.end_byte());
        assert!(named_children(first).iter().all(|n| !n.is_extra()));
        assert_eq!(point("한\n글".as_bytes(), 7), Point { row: 1, column: 3 });
    }
}
