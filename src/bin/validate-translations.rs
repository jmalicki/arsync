//! Translation validation tool for CI
//!
//! Ensures all locale files have matching translation keys.
//! Designed to run in CI to block PRs with incomplete translations.
//!
//! Usage:
//!   cargo run --bin validate-translations
//!
//! Exit codes:
//!   0: All translations complete
//!   1: Missing translations found
//!   2: File I/O error

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process;

/// ANSI color codes for terminal output
struct Colors;
impl Colors {
    const RED: &'static str = "\x1b[0;31m";
    const GREEN: &'static str = "\x1b[0;32m";
    const BLUE: &'static str = "\x1b[0;34m";
    const RESET: &'static str = "\x1b[0m";
}

/// Extract translation keys from a Fluent (.ftl) file
///
/// Keys are lines matching the pattern: `key-name = translation value`
/// Ignores comments (lines starting with #) and empty lines.
fn extract_translation_keys(path: &Path) -> Result<HashSet<String>, std::io::Error> {
    let content = fs::read_to_string(path)?;

    let keys = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();

            // Skip comments and empty lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }

            // Extract key from "key = value" pattern
            if let Some(equals_pos) = trimmed.find('=') {
                let key = trimmed[..equals_pos].trim();
                // Only top-level keys (not indented), valid format
                if !line.starts_with(' ')
                    && !key.is_empty()
                    && key.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                {
                    return Some(key.to_string());
                }
            }
            None
        })
        .collect();

    Ok(keys)
}

/// Load and validate translation keys from a file, exiting on error
fn load_keys_or_exit(path: &Path, _locale_name: &str) -> HashSet<String> {
    match extract_translation_keys(path) {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!(
                "{}ERROR: Failed to read {}: {}{}",
                Colors::RED,
                path.display(),
                e,
                Colors::RESET
            );
            process::exit(2);
        }
    }
}

/// Print missing keys and return true if any found
fn report_missing_keys(missing: &[&String], locale_name: &str, file_path: &Path) -> bool {
    if missing.is_empty() {
        return false;
    }

    println!(
        "{}❌ Missing in {locale_name} ({file_path}): {} keys{}",
        Colors::RED,
        missing.len(),
        Colors::RESET,
        file_path = file_path.display(),
    );
    println!();
    for key in missing {
        println!("  - {key}");
    }
    println!();
    true
}

fn main() {
    let en_us_path = Path::new("locales/en-US/main.ftl");
    let pirate_path = Path::new("locales/qaa/main.ftl");

    println!(
        "{}🔍 Validating translation completeness...{}\n",
        Colors::BLUE,
        Colors::RESET
    );

    // Extract keys from both files
    let en_us_keys = load_keys_or_exit(en_us_path, "en-US");
    let pirate_keys = load_keys_or_exit(pirate_path, "pirate");

    println!(
        "en-US:  {} keys from {}",
        en_us_keys.len(),
        en_us_path.display()
    );
    println!(
        "pirate: {} keys from {}",
        pirate_keys.len(),
        pirate_path.display()
    );
    println!();

    // Find mismatches
    let missing_in_pirate: Vec<_> = en_us_keys.difference(&pirate_keys).collect();
    let missing_in_english: Vec<_> = pirate_keys.difference(&en_us_keys).collect();

    // Report any issues
    let has_errors = report_missing_keys(&missing_in_pirate, "pirate", pirate_path)
        | report_missing_keys(&missing_in_english, "English", en_us_path);

    // Final result
    if has_errors {
        println!(
            "{}═══════════════════════════════════════════════════════════════{}",
            Colors::RED,
            Colors::RESET
        );
        println!(
            "{}  Translation validation FAILED{}",
            Colors::RED,
            Colors::RESET
        );
        println!(
            "{}═══════════════════════════════════════════════════════════════{}",
            Colors::RED,
            Colors::RESET
        );
        println!();
        println!("Fix by adding the missing keys to the appropriate .ftl file.");
        println!("All locales must have identical translation keys.");
        println!();
        process::exit(1);
    } else {
        println!(
            "{}✅ SUCCESS: All translations are complete!{}",
            Colors::GREEN,
            Colors::RESET
        );
        println!();
        println!(
            "Both en-US and pirate have {} matching translation keys.",
            en_us_keys.len()
        );
        println!();
        process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_extract_translation_keys() {
        // Test: Correctly extracts translation keys from FTL format
        let test_ftl = r#"
# Comment line
progress-discovered = Discovered
progress-completed = Completed

# Another comment
error-file-not-found = Not found
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(test_ftl.as_bytes()).unwrap();

        let keys = extract_translation_keys(temp_file.path()).unwrap();

        assert_eq!(keys.len(), 3);
        assert!(keys.contains("progress-discovered"));
        assert!(keys.contains("progress-completed"));
        assert!(keys.contains("error-file-not-found"));
    }

    #[test]
    fn test_extract_ignores_comments() {
        // Test: Comments and empty lines are ignored
        let test_ftl = r#"
# This is a comment
## Another comment

key-one = Value one
# Inline comment
key-two = Value two
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(test_ftl.as_bytes()).unwrap();

        let keys = extract_translation_keys(temp_file.path()).unwrap();

        assert_eq!(keys.len(), 2);
        assert!(keys.contains("key-one"));
        assert!(keys.contains("key-two"));
    }

    #[test]
    fn test_extract_ignores_indented_lines() {
        // Test: Indented lines are ignored (multiline values)
        let test_ftl = r#"
key-one = Value one
    This is a continuation
    Another line
key-two = Value two
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(test_ftl.as_bytes()).unwrap();

        let keys = extract_translation_keys(temp_file.path()).unwrap();

        assert_eq!(keys.len(), 2);
        assert!(keys.contains("key-one"));
        assert!(keys.contains("key-two"));
    }

    #[test]
    fn test_validator_detects_missing_keys() {
        // Test: Validation detects when keys are missing in one locale
        // This is the critical test - ensures the validator actually works!
        //
        // This is linked to the requirement: Validator must catch incomplete translations

        // Create English file with 3 keys
        let en_ftl = r#"
progress-discovered = Discovered
progress-completed = Completed
status-complete = Complete
"#;

        // Create Pirate file with only 2 keys (missing status-complete)
        let pirate_ftl = r#"
progress-discovered = Treasure found, arrr!
progress-completed = Booty secured, matey!
"#;

        let mut en_file = NamedTempFile::new().unwrap();
        let mut pirate_file = NamedTempFile::new().unwrap();

        en_file.write_all(en_ftl.as_bytes()).unwrap();
        pirate_file.write_all(pirate_ftl.as_bytes()).unwrap();

        let en_keys = extract_translation_keys(en_file.path()).unwrap();
        let pirate_keys = extract_translation_keys(pirate_file.path()).unwrap();

        // Verify English has 3 keys
        assert_eq!(en_keys.len(), 3, "English should have 3 keys");

        // Verify Pirate has only 2 keys
        assert_eq!(pirate_keys.len(), 2, "Pirate should have 2 keys");

        // Verify the validator detects the missing key
        let missing: Vec<_> = en_keys.difference(&pirate_keys).collect();
        assert_eq!(missing.len(), 1, "Should detect 1 missing key");
        assert!(
            missing.contains(&&"status-complete".to_string()),
            "Should detect status-complete is missing"
        );

        println!(
            "✅ Validator correctly detected missing translation: {:?}",
            missing
        );
    }

    #[test]
    fn test_validator_detects_extra_keys() {
        // Test: Validation detects when extra keys exist in one locale
        // This ensures we catch typos or orphaned translations

        let en_ftl = r#"
progress-discovered = Discovered
progress-completed = Completed
"#;

        let pirate_ftl = r#"
progress-discovered = Treasure found, arrr!
progress-completed = Booty secured, matey!
status-typo-key = This shouldn't be here
"#;

        let mut en_file = NamedTempFile::new().unwrap();
        let mut pirate_file = NamedTempFile::new().unwrap();

        en_file.write_all(en_ftl.as_bytes()).unwrap();
        pirate_file.write_all(pirate_ftl.as_bytes()).unwrap();

        let en_keys = extract_translation_keys(en_file.path()).unwrap();
        let pirate_keys = extract_translation_keys(pirate_file.path()).unwrap();

        // Verify the validator detects the extra key
        let extra: Vec<_> = pirate_keys.difference(&en_keys).collect();
        assert_eq!(extra.len(), 1, "Should detect 1 extra key");
        assert!(
            extra.contains(&&"status-typo-key".to_string()),
            "Should detect status-typo-key is extra"
        );

        println!("✅ Validator correctly detected extra key: {:?}", extra);
    }
}
