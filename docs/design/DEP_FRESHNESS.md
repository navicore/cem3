# Stale Dependency Audit

## Intent

Several direct dependencies are multiple semver-incompatible majors behind
upstream. `cargo update` can't move them — semver-major bumps need
`Cargo.toml` edits — and Renovate apparently isn't bringing them either
(or its PRs are getting ignored). Until we triage these, the gap quietly
widens and we lose two things: security fixes only delivered in the
current line (rustls/webpki-roots, signal-hook), and the option to adopt
ecosystem crates that have moved on (toml 1.0, bincode 3).

This doc is a triage pass, not a blanket "upgrade everything." We want
to know which we can move now, which are gated, and which we're
deliberately holding.

Crates flagged by `just install`:
`bincode 1→3`, `generic-array 0.14.7→0.14.9`, `hmac 0.12→0.13`,
`pbkdf2 0.12→0.13`, `rand 0.9→0.10`, `sha2 0.10→0.11`,
`signal-hook 0.3→0.4`, `toml 0.8→1.1`, `webpki-roots 0.26→1.0`.

## Constraints

- Don't introduce duplicate versions silently. `cargo tree` already shows
  `webpki-roots 0.26 + 1.0` and `rand_core 0.6 + 0.9` (both pulled by
  RustCrypto). Bumping a leaf must reduce, not add, dupes.
- Don't break the rustls crypto-provider config in `Cargo.toml:80` — the
  comment there pins `ring` and avoids `aws_lc_rs` duplication on purpose.
- Don't bump module versions; releases are the user's job.
- Don't touch the Rust toolchain pin (`rust-toolchain.toml` + the
  Forgejo workflows — both must agree, per ARCHITECTURE.md).
- Each upgrade ships as its own commit with `just ci` green.
- Out of scope: switching serialization format, swapping crypto stacks,
  rewriting our HTTP layer to drop the rustls/webpki-roots seam.

## Approach

Triage into three buckets; act on the first two, document the third.

**Bumped (done):**
- `webpki-roots 0.26 → 1.0` — eliminated the duplicate (ureq's
  transitive was already at 1.0). No code change needed; the
  `TLS_SERVER_ROOTS` constant survived as a re-export.
- `signal-hook 0.3 → 0.4` — diagnostics SIGQUIT handler unaffected;
  iterator API survived the major bump.
- `toml 0.8 → 1.0` — `.parse::<toml::Value>()` no longer compiles;
  use `toml::from_str(...)`. One call site (`compiler/build.rs`).
  The other two (`lint/types.rs`, `ffi/manifest.rs`) were already
  using `toml::from_str` and needed no change.
- `bincode 1 → 2.0.1` — **`bincode 3.0.0` on crates.io is a
  `compile_error!` placeholder linking xkcd 2347, not a real
  release.** 2.0.1 is the actual current line. API moved from
  `serialize/deserialize` to `serde::encode_to_vec` /
  `serde::decode_from_slice`, plus typed `EncodeError` / `DecodeError`
  in place of the old `bincode::Error`. Only `runtime/src/serialize.rs`
  was affected; `ValueSerialize::to_bytes` has no production caller
  so on-disk format compatibility wasn't a constraint.

**Deferred (RustCrypto cohort):**
- `sha2 0.10→0.11`, `hmac 0.12→0.13`, `pbkdf2 0.12→0.13`,
  `rand 0.9→0.10`, `generic-array 0.14.7→0.14.9` — all pinned by
  `aes-gcm 0.10` / `ed25519-dalek 2.x` / `aead 0.5` / `digest 0.10`.
  `generic-array 0.14.9` is a patch bump and `cargo update` still
  refuses (`cargo tree --invert generic-array@0.14.7` shows the
  RustCrypto chain holding it). Bumping our directs would add a
  second copy rather than removing the old. Wait for the next
  RustCrypto cohort release (`aes-gcm 0.11` is the gate).
- `rand_core 0.6 + 0.9` duplicate persists for the same reason;
  not a regression.

**Process gap (one-line follow-up):**
- Renovate is enabled with `config:recommended` but the major-version
  PRs either aren't firing or aren't visible. Verify and, if needed,
  add a `rangeStrategy: "bump"` clause for crates we don't want stuck
  at old majors. Don't pile this into the same PRs.

## Domain Events

- **Produces**: a small series of dep-bump commits (one per crate in the
  bump-now list); a short "Deferred dependencies" section appended to
  `docs/ROADMAP.md` or kept in this doc, naming each gated crate and
  what upstream release unblocks it.
- **Consumes**: `cargo update` (still useful for in-range patches);
  Renovate PRs (we want them firing on majors).
- **Must follow**: when `aes-gcm 0.11` (or whichever release ships the
  RustCrypto bumps) lands, revisit the deferred bucket as a batch
  rather than crate-by-crate — they all move together.

## Checkpoints

- After the bump-now run, `just install` no longer warns about
  `webpki-roots`, `signal-hook`, `toml`, `bincode` (and `generic-array`
  if it made the cut).
- `cargo tree | grep webpki-roots` shows a single version.
- `cargo tree | grep rand_core` still shows two — that's expected and
  documented in the deferred section, not a regression.
- `just ci` is green on every bump commit.
- Examples that touch each crate still run: TLS client
  (`examples/net/tls/client.seq`), anything using `crypto.aes-*`,
  anything using `seq:lint` / FFI manifest, plus the SIGQUIT path.
- One follow-up issue filed against Renovate config if PRs aren't
  reaching us.
