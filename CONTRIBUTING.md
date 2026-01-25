# Contributing to Seq

PRs welcome! Whether it's a bug fix, new feature, documentation improvement, or just a typo fix - all contributions are appreciated.

## Getting Started

1. Fork and clone the repo
2. Enable git hooks to prevent accidental binary commits:
   ```bash
   git config core.hooksPath .githooks
   ```
3. Build and test:
   ```bash
   cargo build
   cargo test --workspace
   ```

## Submitting Changes

1. Create a branch for your changes
2. Write tests for new functionality
3. Ensure all tests pass: `cargo test --workspace`
4. Submit a PR with a clear description of what and why

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Keep PRs focused - one feature or fix per PR

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project: dual-licensed under MIT and Apache 2.0 (your choice).

No CLA, no IP assignment paperwork, no hoops to jump through.

## Questions?

Open an issue or start a discussion. Don't be shy - there are no silly questions.
