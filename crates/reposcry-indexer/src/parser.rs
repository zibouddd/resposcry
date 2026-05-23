use anyhow::Result;
use tracing::warn;

use reposcry_graph::language::Language;
use reposcry_graph::symbol::{Import, ParsedFile, Symbol, TestCase};

pub fn parse_file(path: &str, source: &str) -> Result<ParsedFile> {
    let language = Language::from_extension(path);
    let lang_id = language.as_str().to_string();
    if !language.has_tree_sitter_parser() {
        return Ok(ParsedFile {
            path: path.to_string(),
            language: lang_id,
            symbols: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
            tests: Vec::new(),
            hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            size_bytes: source.len() as u64,
            loc: source.lines().count() as u32,
        });
    }
    let parsed = match language {
        Language::Rust => parse_rust(path, source),
        Language::TypeScript => parse_typescript(path, source),
        Language::JavaScript => parse_javascript(path, source),
        Language::Python => parse_python(path, source),
        _ => ParsedFile {
            path: path.to_string(),
            language: lang_id,
            symbols: Vec::new(),
            imports: Vec::new(),
            calls: Vec::new(),
            tests: Vec::new(),
            hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            size_bytes: source.len() as u64,
            loc: source.lines().count() as u32,
        },
    };
    Ok(ParsedFile {
        hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        size_bytes: source.len() as u64,
        loc: source.lines().count() as u32,
        ..parsed
    })
}

fn usize_to_u32(v: usize) -> u32 {
    v as u32
}

fn node_text<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

fn node_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| node_text(&n, source))
}

fn visibility_from_node(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("visibility")
        .map(|n| node_text(&n, source))
}

fn find_child<'a>(node: &'a tree_sitter::Node, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

fn extract_function_signature(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let name = node_name(node, source)?;
    let vis = visibility_from_node(node, source);
    let prefix = vis.map(|v| format!("{} ", v)).unwrap_or_default();
    let params = node
        .child_by_field_name("parameters")
        .map(|p| node_text(&p, source))
        .unwrap_or_else(|| "()".to_string());
    let return_type = node
        .child_by_field_name("return_type")
        .map(|r| format!(" -> {}", node_text(&r, source)))
        .unwrap_or_default();
    Some(format!("{}fn {}{}{}", prefix, name, params, return_type))
}

fn extract_impl_type(node: &tree_sitter::Node, source: &[u8]) -> String {
    node
        .child_by_field_name("type")
        .map(|n| node_text(&n, source))
        .unwrap_or_else(|| "unknown".to_string())
}

fn extract_impl_methods(
    node: &tree_sitter::Node,
    source: &[u8],
    type_name: &str,
    symbols: &mut Vec<Symbol>,
    path: &str,
) {
    let body = node.child_by_field_name("body").unwrap_or(*node);
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_item" {
            let cname = node_name(&child, source).unwrap_or_else(|| "unknown".to_string());
            let vis = visibility_from_node(&child, source);
            let sig = format!(
                "{} {}::fn {}()",
                vis.as_deref().unwrap_or(""),
                type_name,
                cname
            );
            symbols.push(Symbol {
                id: None,
                file_path: path.to_string(),
                name: format!("{}::{}", type_name, cname),
                kind: "method".into(),
                start_line: usize_to_u32(child.start_position().row + 1),
                end_line: usize_to_u32(child.end_position().row + 1),
                signature: Some(sig),
                visibility: vis,
                doc_comment: None,
            });
        }
    }
}

fn extract_scoped_use(node: &tree_sitter::Node, source: &[u8]) -> String {
    let text = node_text(node, source);
    let text_clean = text.strip_suffix(';').unwrap_or(&text).trim().to_string();
    if text_clean.starts_with("use ") {
        text_clean[4..].to_string()
    } else {
        text_clean
    }
}

fn extract_ts_import(node: &tree_sitter::Node, source: &[u8], path: &str) -> Import {
    let text = node_text(node, source);
    let line = usize_to_u32(node.start_position().row + 1);
    let source_mod = node
        .child_by_field_name("source")
        .map(|s| {
            node_text(&s, source)
                .trim_matches('\'')
                .trim_matches('"')
                .to_string()
        })
        .unwrap_or_else(|| text.clone());
    let imported_names = node
        .child_by_field_name("import_clause")
        .map(|clause| {
            let mut names = Vec::new();
            let mut cursor = clause.walk();
            for child in clause.children(&mut cursor) {
                if child.kind() == "namespace_import" {
                    if let Some(name) = child.child_by_field_name("name") {
                        names.push(format!("* as {}", node_text(&name, source)));
                    }
                } else if let Some(name) = child.child_by_field_name("name") {
                    names.push(node_text(&name, source));
                }
            }
            names
        })
        .unwrap_or_default();
    Import {
        source: path.to_string(),
        target: source_mod.clone(),
        is_relative: text.contains("from") && source_mod.starts_with('.'),
        imported_names,
        line,
    }
}

// ── Rust Parser ──────────────────────────────────────────

fn parse_rust(path: &str, source: &str) -> ParsedFile {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut tests = Vec::new();
    let _calls = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Rust language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            warn!("Failed to parse Rust file: {}", path);
            return empty_parsed(path, "rust");
        }
    };
    let root = tree.root_node();
    let source_bytes = source.as_bytes();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        let kind = node.kind();
        let start = usize_to_u32(node.start_position().row + 1);
        let end = usize_to_u32(node.end_position().row + 1);
        match kind {
            "mod_item" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "module".into(),
                    start_line: start,
                    end_line: end,
                    signature: None,
                    visibility: visibility_from_node(&node, source_bytes),
                    doc_comment: None,
                });
            }
            "struct_item" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let vis = visibility_from_node(&node, source_bytes);
                let sig = format!(
                    "{}struct {}",
                    vis.as_deref()
                        .map(|v| format!("{} ", v))
                        .unwrap_or_default(),
                    name
                );
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name: name.clone(),
                    kind: "struct".into(),
                    start_line: start,
                    end_line: end,
                    signature: Some(sig),
                    visibility: vis,
                    doc_comment: None,
                });
                extract_impl_methods(&node, source_bytes, &name, &mut symbols, path);
            }
            "enum_item" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "enum".into(),
                    start_line: start,
                    end_line: end,
                    signature: None,
                    visibility: visibility_from_node(&node, source_bytes),
                    doc_comment: None,
                });
            }
            "trait_item" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "trait".into(),
                    start_line: start,
                    end_line: end,
                    signature: None,
                    visibility: visibility_from_node(&node, source_bytes),
                    doc_comment: None,
                });
            }
            "function_item" | "function_signature_item" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                if name == "main" {
                    continue;
                }
                let vis = visibility_from_node(&node, source_bytes);
                let attr_start = node.start_byte().saturating_sub(256);
                let attrs = &source[attr_start..node.start_byte()];
                let is_test = attrs.contains("#[test]")
                    || attrs.contains("#[tokio::test]")
                    || attrs.contains("#[async_std::test]");
                let sig = extract_function_signature(&node, source_bytes);
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name: name.clone(),
                    kind: if is_test { "test" } else { "function" }.into(),
                    start_line: start,
                    end_line: end,
                    signature: sig,
                    visibility: vis,
                    doc_comment: None,
                });
                if is_test {
                    tests.push(TestCase {
                        name,
                        file: path.to_string(),
                        line: start,
                        is_async: false,
                    });
                }
            }
            "impl_item" => {
                let type_name = extract_impl_type(&node, source_bytes);
                extract_impl_methods(&node, source_bytes, &type_name, &mut symbols, path);
            }
            "use_declaration" => {
                let target = extract_scoped_use(&node, source_bytes);
                imports.push(Import {
                    source: path.to_string(),
                    target,
                    is_relative: false,
                    imported_names: Vec::new(),
                    line: start,
                });
            }
            _ => {}
        }
    }
    ParsedFile {
        path: path.to_string(),
        language: "rust".into(),
        symbols,
        imports,
        calls: _calls,
        tests,
        hash: String::new(),
        size_bytes: 0,
        loc: 0,
    }
}

// ── TypeScript Parser ────────────────────────────────────

fn parse_typescript(path: &str, source: &str) -> ParsedFile {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let _tests = Vec::new();
    let _calls = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("TS language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            warn!("Failed to parse TS file: {}", path);
            return empty_parsed(path, "typescript");
        }
    };
    let root = tree.root_node();
    let source_bytes = source.as_bytes();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        let kind = node.kind();
        let start = usize_to_u32(node.start_position().row + 1);
        let end = usize_to_u32(node.end_position().row + 1);
        match kind {
            "import_statement" | "import_declaration" => {
                imports.push(extract_ts_import(&node, source_bytes, path));
            }
            "export_statement" => {
                if let Some(inner) = find_child(&node, "function_declaration") {
                    let export_name = node_name(&inner, source_bytes)
                        .unwrap_or_else(|| "unknown".to_string());
                    let export_sig = extract_function_signature(&inner, source_bytes);
                    symbols.push(Symbol {
                        id: None,
                        file_path: path.to_string(),
                        name: export_name,
                        kind: "function".into(),
                        start_line: usize_to_u32(inner.start_position().row + 1),
                        end_line: usize_to_u32(inner.end_position().row + 1),
                        signature: export_sig,
                        visibility: Some("export".into()),
                        doc_comment: None,
                    });
                }
            }
            "function_declaration" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let sig = extract_function_signature(&node, source_bytes);
                let is_component = name
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false);
                let is_handler = matches!(
                    name.as_str(),
                    "GET" | "POST" | "PUT" | "DELETE" | "PATCH"
                );
                let fn_kind = if is_handler {
                    "api_handler"
                } else if is_component {
                    "component"
                } else {
                    "function"
                };
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: fn_kind.into(),
                    start_line: start,
                    end_line: end,
                    signature: sig,
                    visibility: None,
                    doc_comment: None,
                });
            }
            "lexical_declaration" => {
                if let Some(var_decl) = find_child(&node, "variable_declarator") {
                    if let Some(name_node) = var_decl.child_by_field_name("name") {
                        let name = node_text(&name_node, source_bytes);
                        let is_func = find_child(&var_decl, "arrow_function").is_some()
                            || find_child(&var_decl, "function").is_some();
                        if is_func {
                            let is_component = name
                                .chars()
                                .next()
                                .map(|c| c.is_uppercase())
                                .unwrap_or(false);
                            let sig = format!("const {} = fn", name);
                            let kind = if name.starts_with("use") {
                                "hook"
                            } else if is_component {
                                "component"
                            } else {
                                "function"
                            };
                            symbols.push(Symbol {
                                id: None,
                                file_path: path.to_string(),
                                name,
                                kind: kind.into(),
                                start_line: start,
                                end_line: end,
                                signature: Some(sig),
                                visibility: None,
                                doc_comment: None,
                            });
                        }
                    }
                }
            }
            "class_declaration" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let sig = format!("class {}", name);
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "class".into(),
                    start_line: start,
                    end_line: end,
                    signature: Some(sig),
                    visibility: None,
                    doc_comment: None,
                });
            }
            "interface_declaration" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let sig = format!("interface {}", name);
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "interface".into(),
                    start_line: start,
                    end_line: end,
                    signature: Some(sig),
                    visibility: None,
                    doc_comment: None,
                });
            }
            "type_alias_declaration" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let sig = format!("type {}", name);
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "type".into(),
                    start_line: start,
                    end_line: end,
                    signature: Some(sig),
                    visibility: None,
                    doc_comment: None,
                });
            }
            _ => {}
        }
    }
    ParsedFile {
        path: path.to_string(),
        language: "typescript".into(),
        symbols,
        imports,
        calls: _calls,
        tests: _tests,
        hash: String::new(),
        size_bytes: 0,
        loc: 0,
    }
}

// ── JavaScript Parser ────────────────────────────────────

fn parse_javascript(path: &str, source: &str) -> ParsedFile {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let _tests = Vec::new();
    let _calls = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("JS language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            warn!("Failed to parse JS file: {}", path);
            return empty_parsed(path, "javascript");
        }
    };
    let root = tree.root_node();
    let source_bytes = source.as_bytes();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        let kind = node.kind();
        let start = usize_to_u32(node.start_position().row + 1);
        match kind {
            "import_statement" | "import_declaration" => {
                imports.push(extract_ts_import(&node, source_bytes, path));
            }
            "function_declaration" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "function".into(),
                    start_line: start,
                    end_line: usize_to_u32(node.end_position().row + 1),
                    signature: extract_function_signature(&node, source_bytes),
                    visibility: None,
                    doc_comment: None,
                });
            }
            _ => {}
        }
    }
    ParsedFile {
        path: path.to_string(),
        language: "javascript".into(),
        symbols,
        imports,
        calls: _calls,
        tests: _tests,
        hash: String::new(),
        size_bytes: 0,
        loc: 0,
    }
}

// ── Python Parser ────────────────────────────────────────

fn parse_python(path: &str, source: &str) -> ParsedFile {
    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut tests = Vec::new();
    let _calls = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("Python language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => {
            warn!("Failed to parse Python file: {}", path);
            return empty_parsed(path, "python");
        }
    };
    let root = tree.root_node();
    let source_bytes = source.as_bytes();
    for i in 0..root.child_count() {
        let node = root.child(i).unwrap();
        let kind = node.kind();
        let start = usize_to_u32(node.start_position().row + 1);
        match kind {
            "import_statement" | "import_from_statement" => {
                let text = node_text(&node, source_bytes);
                imports.push(Import {
                    source: path.to_string(),
                    target: text.clone(),
                    is_relative: text.starts_with('.'),
                    imported_names: vec![text],
                    line: start,
                });
            }
            "function_definition" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let is_test = name.starts_with("test_");
                let sig = format!("def {}", name);
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name: name.clone(),
                    kind: if is_test { "test" } else { "function" }.into(),
                    start_line: start,
                    end_line: usize_to_u32(node.end_position().row + 1),
                    signature: Some(sig),
                    visibility: None,
                    doc_comment: None,
                });
                if is_test {
                    tests.push(TestCase {
                        name,
                        file: path.to_string(),
                        line: start,
                        is_async: false,
                    });
                }
            }
            "class_definition" => {
                let name = node_name(&node, source_bytes)
                    .unwrap_or_else(|| "unknown".to_string());
                let sig = format!("class {}", name);
                symbols.push(Symbol {
                    id: None,
                    file_path: path.to_string(),
                    name,
                    kind: "class".into(),
                    start_line: start,
                    end_line: usize_to_u32(node.end_position().row + 1),
                    signature: Some(sig),
                    visibility: None,
                    doc_comment: None,
                });
            }
            _ => {}
        }
    }
    ParsedFile {
        path: path.to_string(),
        language: "python".into(),
        symbols,
        imports,
        calls: _calls,
        tests,
        hash: String::new(),
        size_bytes: 0,
        loc: 0,
    }
}

fn empty_parsed(path: &str, language: &str) -> ParsedFile {
    ParsedFile {
        path: path.to_string(),
        language: language.to_string(),
        symbols: Vec::new(),
        imports: Vec::new(),
        calls: Vec::new(),
        tests: Vec::new(),
        hash: String::new(),
        size_bytes: 0,
        loc: 0,
    }
}
