(execute_statement procedure: (object_reference name: (identifier) @exec))
(column_definition . name: (identifier) @column)
; This grammar gives the restored database no field: the identifier after DATABASE.
(restore_statement (keyword_database) . (identifier) @restore)
; MOVE is an option here: name MOVE, the logical file as value, then TO and the OS file.
(option name: (identifier) value: (literal) @move_from (keyword_to) (literal) @move_to)
(option name: (identifier) value: (identifier) @move_from_name (keyword_to))
(assignment left: (field name: (identifier) @assigned))
; A branch ORDER BY is the sibling right after its query_specification here, not a child.
((query_specification (select (top_clause))) . (order_by (order_target (field name: (identifier) @branch_order))))
(set_operation (offset_fetch) @union_offset)
(relation (object_reference name: (identifier) @table))
