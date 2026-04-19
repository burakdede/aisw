use std::path::PathBuf;

fn extract_backtick_spans(markdown: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut in_code = false;
    let mut current = String::new();

    for ch in markdown.chars() {
        if ch == '`' {
            if in_code {
                spans.push(current.clone());
                current.clear();
                in_code = false;
            } else {
                in_code = true;
            }
            continue;
        }

        if in_code {
            current.push(ch);
        }
    }

    spans
}

fn referenced_rs_path(span: &str) -> Option<&str> {
    let trimmed = span.trim();
    if !(trimmed.starts_with("tests/") || trimmed.starts_with("src/")) {
        return None;
    }
    if !trimmed.ends_with(".rs") {
        return None;
    }
    Some(trimmed)
}

fn referenced_test_symbol(span: &str) -> Option<(&str, &str)> {
    let trimmed = span.trim();
    if !trimmed.starts_with("tests/") {
        return None;
    }

    let (path, symbol) = trimmed.split_once("::")?;
    if !path.ends_with(".rs") || symbol.is_empty() {
        return None;
    }
    Some((path, symbol))
}

#[test]
fn acceptance_matrix_references_existing_files_and_tests() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let matrix_path = repo_root.join("docs").join("acceptance-matrix.md");
    let matrix = std::fs::read_to_string(&matrix_path)
        .expect("acceptance-matrix.md should be readable in repo");

    let spans = extract_backtick_spans(&matrix);
    assert!(
        !spans.is_empty(),
        "expected at least one backtick span in {}",
        matrix_path.display()
    );

    for span in spans {
        if let Some(path) = referenced_rs_path(&span) {
            let absolute = repo_root.join(path);
            assert!(
                absolute.exists(),
                "referenced Rust file does not exist in docs/acceptance-matrix.md: {}",
                path
            );
        }

        if let Some((path, symbol)) = referenced_test_symbol(&span) {
            let absolute = repo_root.join(path);
            assert!(
                absolute.exists(),
                "referenced test file does not exist in docs/acceptance-matrix.md: {}",
                path
            );

            let file_contents = std::fs::read_to_string(&absolute).unwrap_or_else(|_| {
                panic!(
                    "referenced test file should be readable: {}",
                    absolute.display()
                )
            });
            let needle = format!("fn {}(", symbol);
            assert!(
                file_contents.contains(&needle),
                "referenced test symbol is missing from {}: {}",
                path,
                symbol
            );
        }
    }
}
