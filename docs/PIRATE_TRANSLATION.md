# Pirate Translation Feature 🏴‍☠️

This document describes the pirate translation feature in arsync, including both documentation translations and runtime language support.

## Overview

arsync now supports full pirate speak translations for both:
1. **Documentation** - All user-facing docs have pirate versions
2. **Runtime Output** - The `--pirate` flag switches all program output to pirate speak

## Documentation Translations

### Files Translated

All user-facing documentation has been translated to pirate speak:

| English Version | Pirate Version |
|----------------|----------------|
| `README.md` | `docs/pirate/README.pirate.md` |
| `docs/CHANGELOG.md` | `docs/pirate/CHANGELOG.pirate.md` |
| `crates/compio-fs-extended/README.md` | `crates/compio-fs-extended/docs/pirate/README.pirate.md` |

### Language Selectors

Each README file includes a language selector at the top:

```markdown
*Read this in other languages: [English](README.md) | [Pirate 🏴‍☠️](docs/pirate/README.pirate.md)*
```

This allows users to easily switch between languages.

### Structure Validation

A test suite (`tests/readme_structure_test.rs`) ensures that:
- Both versions have the same heading structure
- Internal link counts are similar
- External link counts match exactly
- Key sections exist in both versions
- Language selectors are present in both files

Run the test:
```bash
cargo test --test readme_structure_test
```

## Runtime i18n Support

### Architecture

The i18n system is implemented in `src/i18n.rs` with:

1. **Language enum**: `Language::English` and `Language::Pirate`
2. **Translation keys**: Comprehensive enum covering all user-facing messages
3. **Global language setting**: Thread-safe language switching
4. **Translation macro**: `t!` macro for easy access

### Using the `--pirate` Flag

Enable pirate speak for all program output:

```bash
# Normal output (English)
arsync -a --source /data --destination /backup

# Pirate output (arrr!)
arsync -a --source /data --destination /backup --pirate
```

### Example Output Comparison

**English:**
```
Starting arsync v0.1.0
Source directory or file: /data
Destination directory or file: /backup
Complete
Files Completed: 150
Bytes Completed: 1048576
```

**Pirate:**
```
Settin' sail fer plunderin': arsync v0.1.0
Source treasure hold or booty: /data
Destination treasure hold or booty: /backup
Mission complete, arrr!
treasures Plundered: 150
doubloons Plundered: 1048576
```

### Translation Coverage

All user-facing messages are translatable, including:

- **Progress messages**: "Discovered" → "Sighted", "Completed" → "Plundered"
- **Status messages**: "Copying file" → "Plunderin' treasure"
- **Error messages**: "File not found" → "Treasure not found, ye scurvy dog"
- **Info messages**: "Starting copy operation" → "Settin' sail fer plunderin'"
- **CLI help text**: All flag descriptions have pirate translations
- **Units**: "files" → "treasures", "bytes" → "doubloons"

### Adding New Translations

To add a new translatable message:

1. Add a new variant to `TranslationKey` enum in `src/i18n.rs`:
```rust
pub enum TranslationKey {
    // ... existing variants ...
    NewMessage,
}
```

2. Add English and Pirate translations:
```rust
fn english(&self) -> &'static str {
    match self {
        // ... existing translations ...
        Self::NewMessage => "Your message here",
    }
}

fn pirate(&self) -> &'static str {
    match self {
        // ... existing translations ...
        Self::NewMessage => "Yer pirate message here, arrr!",
    }
}
```

3. Use it in code:
```rust
use crate::i18n::TranslationKey;

println!("{}", TranslationKey::NewMessage.get());
```

### Testing i18n

The i18n module includes comprehensive tests:

```bash
cargo test i18n
```

Tests verify:
- Default language is English
- Language can be switched to Pirate
- All keys have both English and Pirate translations
- Translations are non-empty and different

## Implementation Details

### Key Files

- `src/i18n.rs` - Translation infrastructure
- `src/main.rs` - Language initialization based on `--pirate` flag
- `src/progress.rs` - Uses i18n for progress messages
- `src/cli.rs` - Defines `--pirate` flag
- `tests/readme_structure_test.rs` - Validates documentation structure

### Dependencies

- `once_cell` - Thread-safe lazy static for global language setting

### Design Decisions

1. **Global language setting**: Simplifies usage throughout the codebase
2. **Compile-time translations**: No external files needed, zero runtime overhead
3. **Type-safe keys**: Enum prevents typos and enables IDE autocomplete
4. **Thread-safe**: Can be used safely in async/concurrent code

## Pirate Translation Guide

When translating to pirate speak:

### Key Terms

| English | Pirate |
|---------|--------|
| file | treasure |
| directory | cargo hold |
| copy | plunder |
| source | source treasure hold |
| destination | destination treasure hold |
| bytes | doubloons |
| permission denied | ye don't have the key to this chest |
| error | ship's takin' on water |
| owner | captain |
| group | crew |
| symlink | treasure map |
| hardlink | treasure link |
| attributes | treasure markings |

### Style Guidelines

- Use "arrr!" sparingly for emphasis
- Maintain 🏴‍☠️ emoji where appropriate
- Keep technical accuracy despite pirate speak
- Use "ye", "yer", "matey" for second person
- Drop 'g' from -ing words: "plunderin'", "sailin'"
- Use nautical metaphors: "settin' sail", "port", "ship"

## Future Enhancements

Potential additions:

1. More languages (e.g., Spanish, French, Klingon)
2. Environment variable for default language
3. Config file language setting
4. Progress bar text translations
5. Error message detail translations

## Contributing

When adding pirate translations:

1. Ensure structural consistency with English version
2. Maintain all technical references and links
3. Run structure validation tests
4. Keep the pirate spirit alive! 🏴‍☠️

## License

Same as main project (MIT).

---

**Arrr! May yer treasure plunderin' be swift and yer holds be full! 🏴‍☠️**

