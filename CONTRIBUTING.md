# Contributing to PromptHive 🐝

Thank you for your interest in contributing to PromptHive! We welcome contributions from the community and are excited to work with you.

## Code of Conduct

By participating in this project, you agree to abide by our Code of Conduct. Please be respectful and constructive in all interactions.

## Getting Started

1. **Fork the repository** (when available on GitHub)
2. **Clone your fork**:
   ```bash
   git clone https://github.com/YOUR_USERNAME/prompthive
   cd prompthive
   ```
3. **Set up development environment**:
   ```bash
   # Install Rust (if not already installed)
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # Build the project
   cargo build --release
   
   # Run tests
   cargo test --release
   ```

## Ways to Contribute

### 1. Report Bugs
- Check if the issue already exists
- Create a clear, minimal reproduction
- Include system information (OS, Rust version)
- Describe expected vs actual behavior

### 2. Request Features
- Explain the use case
- Provide examples of how it would work
- Consider if it aligns with PromptHive's philosophy

### 3. Submit Prompts
- Create high-quality, reusable prompts
- Test them thoroughly
- Submit to the community registry
- Follow the prompt format guidelines

### 4. Improve Documentation
- Fix typos and clarify explanations
- Add examples and use cases
- Translate documentation
- Create tutorials and guides

### 5. Contribute Code
- Follow Rust best practices
- Maintain sub-80ms performance
- Add tests for new features
- Update documentation as needed

## Development Guidelines

### Performance Standards
- **All operations must complete in <80ms**
- Profile performance impact of changes
- Use `cargo test test_performance` to verify

### Code Style
- Follow standard Rust formatting (`cargo fmt`)
- Use meaningful variable names
- Add comments for complex logic
- Keep functions focused and small

### Testing
- Write unit tests for new functions
- Add integration tests for new commands
- Ensure all tests pass before submitting
- Test on multiple platforms if possible

### Commit Messages
Follow conventional commits:
```
feat: add new clean command
fix: resolve clipboard issue on Linux
docs: update installation instructions
test: add performance benchmarks
refactor: simplify prompt matching logic
```

## Pull Request Process

1. **Create a feature branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes**:
   - Write clean, documented code
   - Add/update tests
   - Update documentation

3. **Test thoroughly**:
   ```bash
   cargo test --release
   cargo check
   cargo clippy
   ```

4. **Submit PR**:
   - Clear description of changes
   - Link to related issues
   - Include examples if applicable

## Prompt Contribution Guidelines

### Format
```markdown
---
id: category/prompt-name
description: Clear, one-line description
tags: [tag1, tag2]
author: YourName (optional)
---

# Prompt Title

Your prompt content here with {placeholders} for variables.
Include clear instructions and expected output format.
```

### Quality Standards
- Clear, actionable instructions
- Reusable with placeholders
- Tested with multiple AI tools
- Professional tone
- No sensitive information

## Community Registry

To submit prompts to the registry:

1. Create your prompt following the format above
2. Test it thoroughly with different inputs
3. Place in appropriate category
4. Submit via pull request or `ph publish`

## Questions?

- Check existing documentation
- Search closed issues
- Ask in discussions
- Join our community chat

## Recognition

Contributors will be recognized in:
- Release notes
- Contributors file
- Community highlights

Thank you for helping make PromptHive better! 🐝