(procedure_body ";" @module.semi)
(procedure_body (statement (query_specification (select (select_expression (term (literal) @module.value))))))
(batch (statement (query_specification (select (select_expression (term (literal) @batch.value))))))
