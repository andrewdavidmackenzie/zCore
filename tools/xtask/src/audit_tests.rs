//! Audit the 1:1 correspondence between Rust `core_tests` and C++ `core-tests-standalone`.
//!
//! This module parses both the C++ test list (`tests/fuchsia-core-tests-from-binary.txt`)
//! and the Rust test source files (`zCore/zircon-tests/src/core_tests/*.rs`) to verify:
//!
//! 1. Every C++ test has exactly one Rust test with a matching `/// C++: TEST(Suite, Name)`
//!    doc comment.
//! 2. Every Rust test's doc comment points to a real C++ test.
//! 3. No two Rust tests point to the same C++ test (no duplicates).
//! 4. The total counts match.
//!
//! It also reports counts for `#[ignore]` tests.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;

/// A Rust test extracted from source.
#[derive(Debug)]
struct RustTest {
    file: String,
    fn_name: String,
    line: usize,
    cpp_ref: Option<String>, // "Suite.TestName"
    is_ignored: bool,
    is_stub: bool, // contains `unimplemented!()`
}

/// Parse all test functions from a single Rust source file.
/// A test function is any `fn` preceded by a `/// C++: TEST(Suite, Name)`
/// or `/// C++: TEST_F(Suite, Name)` doc comment. Extracts the C++ reference,
/// and whether the body contains `unimplemented!()`.
fn parse_rust_tests(path: &Path) -> Vec<RustTest> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let lines: Vec<&str> = content.lines().collect();
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut tests = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Look for fn declarations (test functions are plain fns)
        let fn_name = if let Some(name) = extract_fn_name(trimmed) {
            name
        } else {
            i += 1;
            continue;
        };
        let fn_line = i;

        // Look backward for `/// C++: TEST(Suite, Name)` or `/// C++: TEST_F(Suite, Name)`
        let mut cpp_ref = None;
        // Parse the fn's attribute lines for `#[ignore]` — hardcoding
        // `false` here would let an ignored test slip past the audit's
        // no-ignored-tests policy unnoticed.
        let mut is_ignored = false;
        {
            let mut j = fn_line;
            while j > 0 {
                j -= 1;
                let prev = lines[j].trim();
                if prev.starts_with("///") {
                    if let Some(ref_str) = extract_cpp_ref(prev) {
                        cpp_ref = Some(ref_str);
                    }
                } else if prev.starts_with("#[") {
                    // attributes between doc and fn are ok
                    if is_ignore_attr(prev) {
                        is_ignored = true;
                    }
                } else if prev.starts_with("//") || prev.is_empty() {
                    // comments, blank lines between doc and fn are ok
                    continue;
                } else {
                    break;
                }
            }
        }

        // Skip functions without a C++ reference — they're helpers, not tests
        if cpp_ref.is_none() {
            i += 1;
            continue;
        }

        // Check if body contains `unimplemented!()`
        let is_stub = lines[fn_line..std::cmp::min(lines.len(), fn_line + 6)]
            .iter()
            .any(|l| l.contains("unimplemented!"));

        tests.push(RustTest {
            file: file_name.clone(),
            fn_name,
            line: fn_line + 1, // 1-indexed
            cpp_ref,
            is_ignored,
            is_stub,
        });

        i = fn_line + 1;
    }

    tests
}

/// Extract `Suite.TestName` from a doc comment like `/// C++: TEST(Suite, TestName)`.
/// Also handles `TEST_F(...)` and parameterized tests like
/// `/// C++: TEST(SocketTest/DatagramSocketBadBufferTest, Read/0)`.
fn extract_cpp_ref(line: &str) -> Option<String> {
    // Strip leading `/// ` and any trailing content after the closing `)`
    let content = line.trim_start_matches('/').trim();

    // Match: C++: TEST(Suite, Name) or C++: TEST_F(Suite, Name)
    // The suite can contain `/` (parameterized), the name can contain `/` + digits
    let prefix_patterns = ["C++: TEST_F(", "C++: TEST("];

    for prefix in prefix_patterns {
        if let Some(rest) = content.strip_prefix(prefix) {
            // Find the closing `)` — take only up to first `)`
            let paren_end = rest.find(')')?;
            let inner = &rest[..paren_end];

            // Split on `, ` (comma + space)
            let comma_pos = inner.find(',')?;
            let suite = inner[..comma_pos].trim();
            let name = inner[comma_pos + 1..].trim();

            if !suite.is_empty() && !name.is_empty() {
                return Some(format!("{}.{}", suite, name));
            }
        }
    }

    None
}

/// True for `#[ignore]` and `#[ignore = "..."]` attribute lines.
/// Deliberately does not match lookalikes such as `#[ignored]`.
fn is_ignore_attr(line: &str) -> bool {
    let line = line.trim();
    line == "#[ignore]" || line.starts_with("#[ignore ") || line.starts_with("#[ignore=")
}

/// Extract function name from a line like `fn foo_bar()` or `async fn foo_bar()`.
fn extract_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim();

    // Try `async fn name(` or `fn name(`
    let after_fn = if let Some(rest) = trimmed.strip_prefix("async fn ") {
        Some(rest)
    } else {
        trimmed.strip_prefix("fn ")
    };

    let rest = after_fn?;
    let paren = rest.find('(')?;
    let name = rest[..paren].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(name.to_string())
}

/// Load the C++ test list (one `Suite.TestName` per line, no comments).
fn load_cpp_tests(path: &Path) -> BTreeSet<String> {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read C++ test list at {}: {}", path.display(), e));
    content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Result of the audit.
pub struct AuditResult {
    pub cpp_count: usize,
    pub rust_count: usize,
    pub matched: usize,
    pub duplicates: Vec<(String, Vec<String>)>, // (cpp_ref, [(file:line fn_name), ...])
    pub missing_in_rust: Vec<String>,           // C++ tests with no Rust match
    pub bad_refs: Vec<(String, String, String)>, // (file, fn_name, bad_cpp_ref)
    pub undocumented: Vec<(String, String)>,    // (file, fn_name) — no C++ doc comment
    pub rust_ignored: usize,
    pub rust_stubs: usize,
    // Skip/ignore correspondence
    pub stubs: Vec<(String, String, String)>, // (file, fn_name, cpp_ref) — unimplemented!() tests
    pub ignored_tests: Vec<(String, String, String)>, // #[ignore] tests
    pub petal_ported: usize,                  // Tests ported to petal user-mode binary
    pub unported: Vec<String>, // C++ tests intentionally not ported (see tests/core-tests-unported.txt)
    pub unported_with_rust: Vec<String>, // unported-list entries that DO have a Rust counterpart (inconsistent)
}

/// Run the full audit. Returns `Ok(result)` or `Err(message)` if something
/// is fundamentally wrong (files not found, etc.).
pub fn audit(project_dir: &Path) -> Result<AuditResult, String> {
    let cpp_path = project_dir.join("tests/fuchsia-core-tests-from-binary.txt");
    let core_tests_dir = project_dir.join("zCore/zircon-tests/src/core_tests");

    if !cpp_path.exists() {
        return Err(format!("C++ test list not found: {}", cpp_path.display()));
    }
    if !core_tests_dir.is_dir() {
        return Err(format!(
            "core_tests directory not found: {}",
            core_tests_dir.display()
        ));
    }

    let cpp_tests = load_cpp_tests(&cpp_path);

    // Parse all Rust test files
    let mut all_rust_tests: Vec<RustTest> = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&core_tests_dir)
        .map_err(|e| format!("Failed to read core_tests dir: {}", e))?
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        if path.file_name().is_some_and(|n| n == "mod.rs") {
            continue;
        }
        all_rust_tests.extend(parse_rust_tests(&path));
    }

    // Build mapping: cpp_ref -> list of Rust tests claiming it
    let mut ref_to_rust: HashMap<String, Vec<String>> = HashMap::new();
    let mut matched_cpp: BTreeSet<String> = BTreeSet::new();
    let mut bad_refs: Vec<(String, String, String)> = Vec::new();
    let mut undocumented: Vec<(String, String)> = Vec::new();

    for t in &all_rust_tests {
        if let Some(ref cpp_ref) = t.cpp_ref {
            if cpp_tests.contains(cpp_ref) {
                matched_cpp.insert(cpp_ref.clone());
                ref_to_rust
                    .entry(cpp_ref.clone())
                    .or_default()
                    .push(format!("{}:{} fn {}", t.file, t.line, t.fn_name));
            } else {
                bad_refs.push((t.file.clone(), t.fn_name.clone(), cpp_ref.clone()));
            }
        } else {
            undocumented.push((t.file.clone(), t.fn_name.clone()));
        }
    }

    // Find duplicates
    let mut duplicates: Vec<(String, Vec<String>)> = Vec::new();
    for (cpp_ref, locations) in &ref_to_rust {
        if locations.len() > 1 {
            duplicates.push((cpp_ref.clone(), locations.clone()));
        }
    }
    duplicates.sort_by(|a, b| a.0.cmp(&b.0));

    // Load tests ported to petal (user-mode) — excluded from "missing" check
    let petal_ported: std::collections::HashSet<String> = {
        let petal_file = project_dir.join("tests/petal-ported-tests.txt");
        if petal_file.exists() {
            std::fs::read_to_string(petal_file)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect()
        } else {
            std::collections::HashSet::new()
        }
    };

    // Load intentionally-unported tests (host-threaded suites that
    // cannot run in the no_std kernel test runner) — also excluded
    // from the "missing" check, but reported and cross-checked so the
    // list cannot silently mask a real port.
    let unported: Vec<String> = {
        let unported_file = project_dir.join("tests/core-tests-unported.txt");
        if unported_file.exists() {
            std::fs::read_to_string(unported_file)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                .map(|l| l.trim().to_string())
                .collect()
        } else {
            Vec::new()
        }
    };
    let unported_set: std::collections::HashSet<&String> = unported.iter().collect();
    // Consistency: an unported entry that actually has a Rust
    // counterpart means the list is stale — remove the entry instead.
    let unported_with_rust: Vec<String> = unported
        .iter()
        .filter(|t| matched_cpp.contains(*t))
        .cloned()
        .collect();

    // Find missing (C++ tests with no Rust match, excluding petal-ported
    // and intentionally-unported tests)
    let missing_in_rust: Vec<String> = cpp_tests
        .difference(&matched_cpp)
        .filter(|t| !petal_ported.contains(*t))
        .filter(|t| !unported_set.contains(*t))
        .cloned()
        .collect();

    let rust_ignored = all_rust_tests.iter().filter(|t| t.is_ignored).count();
    let rust_stubs = all_rust_tests.iter().filter(|t| t.is_stub).count();

    // Collect stubs (unimplemented! tests should not exist)
    let stubs: Vec<(String, String, String)> = all_rust_tests
        .iter()
        .filter(|t| t.is_stub)
        .map(|t| {
            (
                t.file.clone(),
                t.fn_name.clone(),
                t.cpp_ref.clone().unwrap_or_default(),
            )
        })
        .collect();

    // #[ignore] tests
    let ignored_tests: Vec<(String, String, String)> = all_rust_tests
        .iter()
        .filter(|t| t.is_ignored && !t.is_stub)
        .map(|t| {
            (
                t.file.clone(),
                t.fn_name.clone(),
                t.cpp_ref.clone().unwrap_or_default(),
            )
        })
        .collect();

    Ok(AuditResult {
        cpp_count: cpp_tests.len(),
        rust_count: all_rust_tests.len(),
        matched: matched_cpp.len(),
        duplicates,
        missing_in_rust,
        bad_refs,
        undocumented,
        rust_ignored,
        rust_stubs,
        stubs,
        ignored_tests,
        petal_ported: petal_ported.len(),
        unported,
        unported_with_rust,
    })
}

/// Print audit results and return true if all checks pass.
pub fn print_and_check(result: &AuditResult) -> bool {
    let mut ok = true;

    println!("=== Core-tests audit ===");
    println!("  C++ tests (binary):      {}", result.cpp_count);
    println!("  Rust tests (core_tests): {}", result.rust_count);
    println!("  Matched 1:1:             {}", result.matched);
    println!("  Rust #[ignore]:          {}", result.rust_ignored);
    println!("  Rust stubs:              {}", result.rust_stubs);

    let petal_count = result.petal_ported;
    let unported_count = result.unported.len();
    let total_rust = result.rust_count + petal_count + unported_count;
    println!("  Petal-ported tests:      {}", petal_count);
    println!("  Intentionally unported:  {}", unported_count);
    if total_rust != result.cpp_count {
        println!(
            "  FAIL: Count mismatch — {} accounted tests ({} kernel + {} petal + {} unported) vs {} C++ tests",
            total_rust, result.rust_count, petal_count, unported_count, result.cpp_count
        );
        ok = false;
    }

    if (result.matched + petal_count + result.unported.len()) != result.cpp_count {
        println!(
            "  FAIL: Only {}/{} C++ tests have a Rust match",
            result.matched, result.cpp_count
        );
        ok = false;
    }

    if !result.unported_with_rust.is_empty() {
        println!(
            "  FAIL: {} unported-list entries have a Rust counterpart (stale list):",
            result.unported_with_rust.len()
        );
        for t in &result.unported_with_rust {
            println!("    {t}");
        }
        println!("    (remove them from tests/core-tests-unported.txt)");
        ok = false;
    }

    if !result.duplicates.is_empty() {
        println!(
            "  FAIL: {} C++ tests claimed by multiple Rust tests:",
            result.duplicates.len()
        );
        for (cpp_ref, locations) in &result.duplicates {
            println!("    {cpp_ref}:");
            for loc in locations {
                println!("      {loc}");
            }
        }
        ok = false;
    }

    if !result.missing_in_rust.is_empty() {
        println!(
            "  FAIL: {} C++ tests have no Rust counterpart:",
            result.missing_in_rust.len()
        );
        for t in result.missing_in_rust.iter().take(20) {
            println!("    {t}");
        }
        if result.missing_in_rust.len() > 20 {
            println!("    ... and {} more", result.missing_in_rust.len() - 20);
        }
        ok = false;
    }

    if !result.bad_refs.is_empty() {
        println!(
            "  FAIL: {} Rust tests reference non-existent C++ tests:",
            result.bad_refs.len()
        );
        for (file, fn_name, cpp_ref) in &result.bad_refs {
            println!("    {file} fn {fn_name} -> {cpp_ref}");
        }
        ok = false;
    }

    if !result.undocumented.is_empty() {
        println!(
            "  FAIL: {} Rust tests have no /// C++: TEST(...) doc comment:",
            result.undocumented.len()
        );
        for (file, fn_name) in result.undocumented.iter().take(20) {
            println!("    {file} fn {fn_name}");
        }
        if result.undocumented.len() > 20 {
            println!("    ... and {} more", result.undocumented.len() - 20);
        }
        ok = false;
    }

    // No stubs allowed — every test must have a real implementation
    if !result.stubs.is_empty() {
        println!(
            "  FAIL: {} Rust tests contain unimplemented!() — stubs are not allowed:",
            result.stubs.len()
        );
        for (file, fn_name, cpp_ref) in &result.stubs {
            println!("    {file} fn {fn_name} -> {cpp_ref}");
        }
        ok = false;
    }

    // #[ignore] audit
    if !result.ignored_tests.is_empty() {
        println!(
            "  FAIL: {} Rust tests are #[ignore]:",
            result.ignored_tests.len()
        );
        for (file, fn_name, cpp_ref) in &result.ignored_tests {
            println!("    {file} fn {fn_name} -> {cpp_ref}");
        }
        println!("    (un-ignore the Rust test or remove it)");
        ok = false;
    }

    if ok {
        println!("  OK: Perfect 1:1 correspondence.");
    }

    ok
}

/// Entry point called from the xtask subcommand.
pub fn run(project_dir: &Path) {
    let result = audit(project_dir).unwrap_or_else(|e| {
        panic!("Audit failed: {}", e);
    });
    if !print_and_check(&result) {
        panic!("Core-tests audit failed — see errors above");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_cpp_ref_basic() {
        assert_eq!(
            extract_cpp_ref("/// C++: TEST(FifoTest, ReadBadBuffer)"),
            Some("FifoTest.ReadBadBuffer".to_string())
        );
    }

    #[test]
    fn test_extract_cpp_ref_test_f() {
        assert_eq!(
            extract_cpp_ref("/// C++: TEST_F(JobGetInfoTest, InfoJobChildrenOnSelfSucceeds)"),
            Some("JobGetInfoTest.InfoJobChildrenOnSelfSucceeds".to_string())
        );
    }

    #[test]
    fn test_extract_cpp_ref_parameterized() {
        assert_eq!(
            extract_cpp_ref("/// C++: TEST(SocketTest/DatagramSocketBadBufferTest, Read/0)"),
            Some("SocketTest/DatagramSocketBadBufferTest.Read/0".to_string())
        );
    }

    #[test]
    fn test_extract_cpp_ref_with_trailing() {
        assert_eq!(
            extract_cpp_ref("/// C++: TEST(Threads, Basics) — basic thread test"),
            Some("Threads.Basics".to_string())
        );
    }

    #[test]
    fn test_extract_cpp_ref_not_a_ref() {
        assert_eq!(extract_cpp_ref("/// Some regular doc comment"), None);
        assert_eq!(extract_cpp_ref("/// C++: see foo.cc"), None);
    }

    #[test]
    fn test_parse_rust_tests_detects_ignore() {
        // End-to-end: the backward scan must record `#[ignore]` on a
        // C++-referenced test fn (attributes sit between the doc
        // comment and the fn — the case the hardcoded `false` missed).
        let dir = std::env::temp_dir().join("xtask_audit_ignore_test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample_tests.rs");
        fs::write(
            &path,
            "// sample\n\n/// C++: TEST(Foo, Bar)\n#[test]\n#[ignore = \"flaky\"]\nfn foo_bar() {\n    assert!(true);\n}\n\n/// C++: TEST(Foo, Baz)\n#[test]\nfn foo_baz() {\n    assert!(true);\n}\n",
        )
        .unwrap();
        let tests = parse_rust_tests(&path);
        assert_eq!(tests.len(), 2);
        let bar = tests.iter().find(|t| t.fn_name == "foo_bar").unwrap();
        assert!(bar.is_ignored);
        assert!(!bar.is_stub);
        let baz = tests.iter().find(|t| t.fn_name == "foo_baz").unwrap();
        assert!(!baz.is_ignored);
    }

    #[test]
    fn test_is_ignore_attr() {
        assert!(is_ignore_attr("#[ignore]"));
        assert!(is_ignore_attr("#[ignore = \"flaky on CI\"]"));
        assert!(is_ignore_attr("  #[ignore]"));
        assert!(!is_ignore_attr("#[ignored]"));
        assert!(!is_ignore_attr("#[test]"));
        assert!(!is_ignore_attr("// #[ignore]"));
    }

    #[test]
    fn test_extract_fn_name() {
        assert_eq!(
            extract_fn_name("fn foo_bar() {"),
            Some("foo_bar".to_string())
        );
        assert_eq!(
            extract_fn_name("async fn baz_qux() {"),
            Some("baz_qux".to_string())
        );
        assert_eq!(extract_fn_name("let x = 5;"), None);
    }
}
