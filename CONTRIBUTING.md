# Contributing to Seq

PRs welcome! Whether it's a bug fix, new feature, documentation improvement, or just a typo fix - all contributions are appreciated.

## Getting Started

1. Fork and clone the repo
2. Install [just](https://github.com/casey/just) (our command runner)
3. Enable git hooks to prevent accidental binary commits:
   ```bash
   git config core.hooksPath .githooks
   ```
4. Build and test:
   ```bash
   just build
   just test
   ```
5. See all available commands:
   ```bash
   just
   ```

## Submitting Changes

1. Create a branch for your changes
2. Write tests for new functionality
3. **Before creating a PR**, run the full CI suite locally:
   ```bash
   just ci
   ```
   This runs the exact same checks as GitHub Actions - if it passes locally, CI will pass.
4. Submit a PR with a clear description of what and why

## Code Style

- Run `just fmt` before committing
- Run `just lint` and address warnings
- Keep PRs focused - one feature or fix per PR

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project: dual-licensed under MIT and Apache 2.0 (your choice).

No CLA, no IP assignment paperwork, no hoops to jump through.

## Questions?

Open an issue or start a discussion. Don't be shy - there are no silly questions.
