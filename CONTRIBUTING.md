# Contributing to PromptHive

First off, thank you for considering contributing to PromptHive! It's people like you that make PromptHive such a great tool for the developer community.

## Code of Conduct

This project and everyone participating in it is governed by our Code of Conduct. By participating, you are expected to uphold this code. Please be kind, constructive, and professional in all interactions.

## How Can I Contribute?

### 🐛 Reporting Bugs

Before creating bug reports, please check existing issues as you might find out that you don't need to create one. When you are creating a bug report, please include as many details as possible:

**Bug Report Template:**
```markdown
**Describe the bug**
A clear and concise description of what the bug is.

**To Reproduce**
Steps to reproduce the behavior:
1. Run command '...'
2. See error

**Expected behavior**
What you expected to happen.

**Actual behavior**
What actually happened.

**Environment:**
- OS: [e.g. macOS 14.0]
- PromptHive version: [run `ph --version`]
- Rust version: [run `rustc --version`]

**Additional context**
Any other context about the problem.
```

### 💡 Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. Before creating enhancement suggestions, please check the existing issues and discussions.

**Enhancement Template:**
```markdown
**Is your feature request related to a problem?**
A clear description of what the problem is.

**Describe the solution you'd like**
A clear description of what you want to happen.

**Describe alternatives you've considered**
Any alternative solutions or features you've considered.

**Additional context**
Any other context or screenshots.
```

### 🔧 Your First Code Contribution

Unsure where to begin? Look for these tags:
- `good first issue` - Good for newcomers
- `help wanted` - Extra attention needed
- `easy` - Should be simple to implement

### 📝 Pull Requests

1. **Fork the repo** and create your branch from `main`
2. **Write code** following our style guide
3. **Add tests** for any new functionality
4. **Ensure tests pass** with `cargo test`
5. **Run lints** with `cargo clippy`
6. **Format code** with `cargo fmt`
7. **Update docs** if needed
8. **Submit PR** with clear description

## Development Setup

### Prerequisites
- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Git
- A terminal emulator

### Local Development
```bash
# Clone your fork
git clone https://github.com/joryeugene/prompthive
cd prompthive

# Add upstream remote
git remote add upstream https://github.com/ORIGINAL_OWNER/prompthive

# Install dependencies and build
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- help

# Run specific command
cargo run -- new test-prompt "This is a test"
```

### Running Tests
```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run integration tests only
cargo test --test '*'

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html
```

## Style Guide

### Rust Code Style

We follow standard Rust conventions:
- Use `cargo fmt` before committing
- Follow `cargo clippy` suggestions
- Use meaningful variable names
- Keep functions focused and small
- Document public APIs

**Example:**
```rust
/// Creates a new prompt with the given name and content
/// 
/// # Arguments
/// * `name` - The name of the prompt
/// * `content` - The prompt content
/// 
/// # Returns
/// * `Result<()>` - Success or error
pub fn create_prompt(name: &str, content: &str) -> Result<()> {
    // Implementation
}
```

### Commit Messages

Follow conventional commits:
```
feat: add new variable system
fix: resolve parsing error in templates
docs: update installation instructions
test: add tests for compose command
refactor: simplify prompt storage logic
perf: optimize search performance
```

### Documentation

- Update README.md for user-facing changes
- Add inline documentation for public functions
- Include examples in documentation
- Keep language clear and concise

## Testing

### Test Categories

1. **Unit Tests** - Test individual functions
2. **Integration Tests** - Test command workflows
3. **Performance Tests** - Verify <80ms operations
4. **Doc Tests** - Ensure examples work

### Writing Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_creation() {
        let result = create_prompt("test", "content");
        assert!(result.is_ok());
    }

    #[test]
    fn test_performance() {
        let start = std::time::Instant::now();
        let _ = list_prompts();
        assert!(start.elapsed().as_millis() < 80);
    }
}
```

## Community Prompt Packs

Have an awesome prompt collection? Share it with the community!

### Creating a Prompt Pack

1. **Structure your repository:**
```
awesome-prompts-coding/
├── README.md
├── backend/
│   ├── api-design.md
│   ├── database-schema.md
│   └── error-handling.md
├── frontend/
│   ├── component-design.md
│   └── state-management.md
└── testing/
    ├── unit-tests.md
    └── integration-tests.md
```

2. **Add metadata to prompts:**
```markdown
---
name: api-design
description: Design REST APIs following best practices
variables:
  - resource: The resource name
  - operations: CRUD operations needed
tags: [backend, api, rest]
---

Design a REST API for {resource} supporting {operations}...
```

3. **Create clear README:**
```markdown
# Awesome Coding Prompts

A collection of prompts for software development.

## Installation
```bash
git clone https://github.com/YOUR/awesome-prompts-coding
ph import ./awesome-prompts-coding
```

## Prompts Included
- `backend/api-design` - REST API design
- `frontend/component-design` - React component patterns
...
```

4. **Submit to awesome-prompthive list** (coming soon)

## Release Process

Releases are automated via GitHub Actions when a tag is pushed:

```bash
git tag v1.2.3
git push origin v1.2.3
```

This will:
1. Run all tests
2. Build binaries for all platforms
3. Create GitHub release
4. Publish to crates.io

## Recognition

Contributors are recognized in:
- The README.md contributors section
- Release notes
- The `CONTRIBUTORS` file

## Questions?

Feel free to:
- Open a discussion for general questions
- Ask in issues for specific problems
- Reach out to maintainers

## Financial Support

If you want to support the project financially:
- GitHub Sponsors (coming soon)
- Buy the maintainers a coffee ☕

Thank you for making PromptHive better! 🐝