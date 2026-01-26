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

## What Good Code Looks Like

Could a new contributor read this file and make a correct change without asking questions? That's the bar.

**Structure**
- The first 50 lines (module doc + imports) tell you what the file does
- Functions are ordered logically (public API at top, helpers below)
- No function requires scrolling more than a screen to understand

**Naming**
- Function names say what they do, not how (`validate_effect` not `check_and_maybe_return_error`)
- Variable names reveal intent (`remaining_args` not `v2`)
- Consistent conventions throughout

**No Surprises**
- Every public function is actually used externally
- No commented-out code blocks
- No stale TODOs or FIXMEs
- Functions do what their names suggest

**Single Responsibility**
- Each function does one thing
- No "utility" functions that handle 5 unrelated cases
- Consistent error handling (`unwrap()` vs `?` used deliberately, not randomly)

**Tests**
- Complex logic has tests nearby
- Edge cases mentioned in comments have corresponding tests

**What We Don't Care About**
- Perfect documentation on every helper function
- Maximum DRY - some repetition is fine if it's clearer
- Clever abstractions - straightforward beats clever

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project: dual-licensed under MIT and Apache 2.0 (your choice).

No CLA, no IP assignment paperwork, no hoops to jump through.

## Questions?

Open an issue or start a discussion. Don't be shy - there are no silly questions.
