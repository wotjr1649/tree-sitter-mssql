fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("usage: tree-check-driver INPUT");
    let source = std::fs::read(path).expect("cannot read input");
    tree_sitter_mssql_fuzz::check(&source);
}
