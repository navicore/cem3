# mdbook-assets

Static assets served by the mdbook documentation build (configured in `../book.toml`).

## Why these files exist

### `mermaid.min.js`, `mermaid-init.js`

Vendored copies of the [mermaid.js](https://mermaid.js.org/) runtime and the
mdbook initializer. They are loaded via `additional-js` in `book.toml` so the
generated book can render mermaid diagrams (e.g. in `docs/ARCHITECTURE.md`)
without depending on a CDN at view time.

These files are produced by `mdbook-mermaid install`. To refresh them, run that
command and move the resulting files back into this directory, then update
`additional-js` paths in `book.toml` if names change.

The `[preprocessor.mermaid]` entry in `book.toml` invokes `mdbook-mermaid` at
build time to translate fenced ` ```mermaid ` blocks into the markup these
scripts render.
