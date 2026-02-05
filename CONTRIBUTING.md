# Contributing to Simple Notes Desktop

Thank you for your interest in contributing to Simple Notes Desktop! 🎉

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Code Style](#code-style)
- [Commit Messages](#commit-messages)
- [Pull Requests](#pull-requests)
- [Reporting Issues](#reporting-issues)

## Code of Conduct

Please be respectful and considerate in all interactions. We welcome contributors of all skill levels.

## Getting Started

### Prerequisites

- Node.js 20+
- pnpm 9+
- Rust (stable)
- Platform-specific dependencies (see [BUILDING.md](BUILDING.md))

### Setup

```bash
# Clone the repository
git clone https://github.com/inventory69/simple-notes-desktop.git
cd simple-notes-desktop

# Install dependencies
pnpm install

# Start development server
pnpm dev
```

For detailed build instructions, see [BUILDING.md](BUILDING.md).

## Development Workflow

1. **Fork** the repository
2. **Create a branch** for your feature or fix:
   ```bash
   git checkout -b feature/amazing-feature
   # or
   git checkout -b fix/issue-123
   ```
3. **Make your changes**
4. **Test your changes** locally
5. **Commit** with a meaningful message
6. **Push** to your fork
7. **Open a Pull Request**

## Code Style

### JavaScript

- ES6+ with ES-Modules
- No semicolons (project convention)
- Single quotes for strings
- Use `const` and `let`, never `var`

### Rust

- Follow standard Rust conventions
- Run `cargo fmt` before committing
- Run `cargo clippy` to catch common issues

### CSS

- Use CSS custom properties (variables)
- Follow BEM-like naming for classes
- Mobile-first approach

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, etc.)
- `refactor`: Code refactoring
- `test`: Adding or updating tests
- `chore`: Maintenance tasks

### Examples

```
feat(editor): add markdown live preview
fix(sync): resolve WebDAV connection timeout
docs: update installation instructions
chore: update dependencies
```

## Pull Requests

### Before Submitting

- [ ] Test your changes locally
- [ ] Run `pnpm test` to ensure tests pass
- [ ] Update documentation if needed
- [ ] Add tests for new features

### PR Description

Please include:
- **What** the PR does
- **Why** the change is needed
- **How** to test it
- Screenshots (for UI changes)

### Review Process

1. A maintainer will review your PR
2. Address any requested changes
3. Once approved, it will be merged

## Reporting Issues

### Bug Reports

Please include:
- Operating system and version
- App version
- Steps to reproduce
- Expected vs actual behavior
- Screenshots or logs if applicable

### Feature Requests

Please describe:
- The problem you're trying to solve
- Your proposed solution
- Alternative solutions you've considered

## Questions?

Feel free to open an issue for questions or join discussions in existing issues.

---

Thank you for contributing! 🙏
