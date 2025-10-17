# /docs

Build and optionally open project documentation.

- open (boolean, optional): Open docs in browser after building (default: true)
- private (boolean, optional): Include private items in documentation (default: false)

```bash
/docs true false   # Build and open public docs
/docs true true    # Build and open all docs including private items
```

Common patterns:
- Build and open: `cargo doc --open`
- Include private: `cargo doc --document-private-items --open`
- All features: `cargo doc --all-features --open`
- No deps: `cargo doc --no-deps --open`
- Specific package: `cargo doc -p <package-name> --open`

Documentation features:
- **Public API docs**: For library users
- **Private docs**: Internal implementation details
- **Examples**: Code examples in doc comments
- **Cross-references**: Links between items

Before releasing:
1. Build docs: `/docs true false`
2. Review API documentation
3. Check for broken links
4. Verify examples compile: `cargo test --doc`
5. Update README if API changed

Documentation best practices:
- Write doc comments for public items
- Include examples in doc comments
- Use `///` for item docs, `//!` for module docs
- Add examples that compile: ` ```rust`
- Link to related items: `[`OtherType`]`

Output location:
- `target/doc/` - Generated documentation
- `target/doc/<crate>/index.html` - Entry point

CI Integration:
- Consider publishing docs to GitHub Pages
- Document breaking changes in CHANGELOG

