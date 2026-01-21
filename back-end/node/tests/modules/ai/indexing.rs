use std::fs;
use std::io::Write;
use swiftide::indexing::loaders::FileLoader;
use tempfile::TempDir;

/// Helper to get path string from a node
fn get_path_str(node: &swiftide::indexing::TextNode) -> String {
    node.path.to_string_lossy().to_string()
}

/// Test that FileLoader loads files from a directory
#[test]
fn test_file_loader_loads_files() {
    // Create temp directory with test files
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create test files with different extensions
    let files = vec![
        ("test.md", "# Test Markdown\n\nSome content here."),
        ("readme.txt", "This is a plain text file with content."),
        ("code.rs", "fn main() { println!(\"Hello\"); }"),
        ("data.json", r#"{"key": "value", "number": 42}"#),
    ];

    for (name, content) in &files {
        let file_path = dir_path.join(name);
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    // Create FileLoader WITHOUT extension filter (should load all files)
    let loader = FileLoader::new(dir_path);
    let nodes = loader.list_nodes();

    println!("Found {} nodes from temp dir", nodes.len());
    for node in &nodes {
        println!("  - {}", get_path_str(node));
    }

    // Should load all 4 files
    assert_eq!(
        nodes.len(),
        4,
        "FileLoader should load all 4 files when no extension filter is set"
    );
}

/// Test FileLoader with extension filter
#[test]
fn test_file_loader_with_extension_filter() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create test files
    fs::write(dir_path.join("test.md"), "# Markdown").unwrap();
    fs::write(dir_path.join("test.txt"), "Plain text").unwrap();
    fs::write(dir_path.join("test.rs"), "fn main() {}").unwrap();

    // Filter to only markdown files
    let loader = FileLoader::new(dir_path).with_extensions(&["md"]);
    let nodes = loader.list_nodes();

    assert_eq!(nodes.len(), 1, "Should only load .md files");
    let path_str = get_path_str(&nodes[0]);
    assert!(
        path_str.ends_with("test.md"),
        "Should load the markdown file, got: {}",
        path_str
    );
}

/// Test FileLoader with specs directory specifically
/// This tests the actual specs directory to understand why it might not be loading
#[test]
fn test_file_loader_specs_directory() {
    let specs_path = "/Users/julian/Documents/Code/Flow Network/Flow/specs";

    // Skip if path doesn't exist (CI environments)
    if !std::path::Path::new(specs_path).exists() {
        eprintln!("Skipping test - specs directory not found");
        return;
    }

    let loader = FileLoader::new(specs_path);
    let nodes = loader.list_nodes();

    println!(
        "Found {} nodes from specs directory at {}",
        nodes.len(),
        specs_path
    );
    for node in &nodes {
        println!("  - {}", get_path_str(node));
    }

    // The specs directory should have markdown files
    assert!(
        nodes.len() > 0,
        "FileLoader should find files in specs directory"
    );
}

/// Test that hidden files (starting with .) are handled correctly
#[test]
fn test_file_loader_hidden_files() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create regular and hidden files
    fs::write(dir_path.join("regular.md"), "Regular file").unwrap();
    fs::write(dir_path.join(".hidden.md"), "Hidden file").unwrap();
    fs::write(dir_path.join(".gitignore"), "*.log").unwrap();

    let loader = FileLoader::new(dir_path);
    let nodes = loader.list_nodes();

    println!("Found {} nodes", nodes.len());
    for node in &nodes {
        println!("  - {}", get_path_str(node));
    }

    // The ignore crate typically skips hidden files
    // This test documents the actual behavior
    let regular_count = nodes
        .iter()
        .filter(|n| {
            let path_str = get_path_str(n);
            !path_str.is_empty() && !std::path::Path::new(&path_str)
                .file_name()
                .map(|f| f.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
        })
        .count();

    assert!(regular_count >= 1, "Should load at least the regular file");
}

/// Test FileLoader with directory containing subdirectories
#[test]
fn test_file_loader_recursive() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Create files in root and subdirectory
    fs::write(dir_path.join("root.md"), "Root file").unwrap();

    let sub_dir = dir_path.join("subdir");
    fs::create_dir(&sub_dir).unwrap();
    fs::write(sub_dir.join("nested.md"), "Nested file").unwrap();

    let loader = FileLoader::new(dir_path);
    let nodes = loader.list_nodes();

    println!("Found {} nodes (recursive)", nodes.len());
    for node in &nodes {
        println!("  - {}", get_path_str(node));
    }

    // Should find both files
    assert_eq!(nodes.len(), 2, "FileLoader should recursively find files");
}

/// Test that .gitignore patterns are respected
/// Note: The ignore crate requires a git repository for .gitignore to work
#[test]
fn test_file_loader_respects_gitignore() {
    let temp_dir = TempDir::new().unwrap();
    let dir_path = temp_dir.path();

    // Initialize a git repository - required for .gitignore to be respected
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir_path)
        .output()
        .expect("Failed to initialize git repo");

    // Create a .gitignore that ignores .log files and ignored/ directory
    fs::write(dir_path.join(".gitignore"), "*.log\nignored/").unwrap();

    // Create files that should and shouldn't be ignored
    fs::write(dir_path.join("included.md"), "Included").unwrap();
    fs::write(dir_path.join("excluded.log"), "Excluded by pattern").unwrap();

    let ignored_dir = dir_path.join("ignored");
    fs::create_dir(&ignored_dir).unwrap();
    fs::write(ignored_dir.join("also_excluded.md"), "In ignored dir").unwrap();

    let loader = FileLoader::new(dir_path);
    let nodes = loader.list_nodes();

    println!("Found {} nodes with .gitignore", nodes.len());
    for node in &nodes {
        println!("  - {}", get_path_str(node));
    }

    // Should only find the included file (and possibly .gitignore itself)
    let md_files: Vec<_> = nodes
        .iter()
        .filter(|n| {
            let path_str = get_path_str(n);
            path_str.ends_with(".md")
        })
        .collect();

    assert_eq!(
        md_files.len(),
        1,
        "Should only find included.md (not the one in ignored/ dir)"
    );
}
