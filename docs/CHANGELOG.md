*Read this in other languages: [English](CHANGELOG.md) | [Pirate 🏴‍☠️](pirate/CHANGELOG.pirate.md)*

---

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial project structure
- Basic CLI interface
- CI/CD pipeline setup
- Comprehensive testing framework
- Documentation structure

### Changed
- Nothing yet

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- Nothing yet

### Security
- Nothing yet

## [0.1.0] - TBD

### Added
- Basic file copying with io_uring
- Metadata preservation (ownership, permissions)
- Progress tracking
- Command-line interface
- Error handling framework

### Changed
- Nothing yet

### Deprecated
- Nothing yet

### Removed
- Nothing yet

### Fixed
- Nothing yet

### Security
- Nothing yet

---

## Release Process

1. Update version in `Cargo.toml`
2. Update this `CHANGELOG.md`
3. Create release branch: `git checkout -b release/v0.1.0`
4. Tag release: `git tag v0.1.0`
5. Push tag: `git push origin v0.1.0`
6. GitHub Actions will automatically build and release
