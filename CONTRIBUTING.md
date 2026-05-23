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

- ES6+ with ES-Modules (`"type": "module"`)
- **Formatting & linting are enforced by Biome** — run `pnpm lint:fix`. The config
  (`biome.json`) requires: **semicolons always**, single quotes, trailing commas, 2-space
  indent, 120-column width, and always-parenthesized arrow params. Imports are auto-organized.
- Use `const`/`let`, never `var`

### Rust

- Follow standard Rust conventions
- Run `cargo fmt` before committing
- Run `cargo clippy` to catch common issues

### CSS

- Use CSS custom properties (variables)
- Follow BEM-like naming for classes
- Mobile-first approach

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/), kept **consistent with
the sibling Android app** ([simple-notes-sync](https://github.com/inventory69/simple-notes-sync)):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style/formatting only (no behavior change)
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding or updating tests
- `build`: Build system, bundling, packaging (Tauri/Vite/AUR)
- `ci`: CI/CD workflows
- `chore`: Maintenance (deps, tooling, version bumps)
- `revert`: Reverting a previous commit
- `release`: Release/version commits (e.g. `release: v0.4.0`) — mirrors the Android app's usage

### Common scopes

Use a scope when it sharpens the message. Scopes seen across both apps:
`editor`, `sync`, `checklist`, `ui`, `linux`, `ci`, `i18n`. Match the affected area; invent a new
scope only when the existing ones don't fit.

### Examples

```
feat(editor): add markdown live preview
fix(sync): preserve Android-only note fields (color/labels) on save
fix(linux): present() window on Wayland to fix frozen titlebar
perf(sync): batch PROPFIND to avoid N+1 GETs
docs: update installation instructions
chore: bump version to 0.5.0
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
