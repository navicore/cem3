# Tagged Int + Bitwise Honesty

## Intent

Seq's `Int` is 63-bit (tagged with the low bit set), but the surface lies about
it: `int-bits` returns 64, the language guide's shift example
`-1 1 shr  # 9223372036854775807` is wrong (the real result is `-1`), and the
bitwise builtins (`shl`, `shr`, `band`, etc.) silently corrupt values that
would set bit 62. This caused `int->bits` of a negative to infinite-loop —
which is how the gap was found.

Goal: make the advertised contract match the actual contract so users do not
write code based on a 64-bit mental model when the runtime is 63-bit. The
63-bit choice is intentional and load-bearing; this is not about changing it.

## Constraints

- Tagged-pointer integers stay (performance, established).
- `band`/`bor`/`bxor`/`bnot` semantics within the 63-bit range are unchanged.
- No BigInt; out of scope.
- Existing `0xFF band`, `1 30 shl` style code keeps working.
- `test-bitwise.seq` and stdlib word semantics stay backwards-compatible.

## Approach

Five pieces, two land immediately, three are a follow-up landing together:

**Now (no behavior change):**

1. **Doc fix.** Update `docs/language-guide.md`: correct the `-1 1 shr`
   example, and add a "63-bit Int model" call-out next to the bitwise table
   so readers see the limit at the same time as the operators. Also
   cross-link from `STDLIB_REFERENCE.md`.
2. **Test coverage.** `test-bitwise.seq` only exercises positive operands.
   Add negative-operand cases for `shl`, `shr`, `band`, `bxor` that pin the
   *current* (silent-truncate) behavior. These will need updating when
   piece 4 lands, but pinning today's behavior is what would have caught
   the `int->bits` regression earlier.

**Follow-up (single CHANGELOG entry):**

3. **Truthful constants.** Change `int-bits` to return 63. Update the doc
   tables in lockstep. If a 64 value is genuinely needed somewhere, add
   `int-bits-storage` — but audit first; it likely is not.
4. **Defined shift overflow.** When `shl`/`shr` would produce a result that
   doesn't fit in [-2^62, 2^62-1], return 0 (matches the existing
   shift-by-64 sentinel) instead of silently retag-truncating. The
   `(value as u64) << 1 | 1` path that loses bit 63 is the worst-case
   outcome today; a defined trap is strictly better.
5. **(Optional) Lint.** A pattern that flags `shr`/`shl` on operands
   reachable from a negative literal or `bnot`. Cheap; targets the exact
   shape that bit `int->bits` here.

## Domain Events

- **Doc fix lands** → no behavior change downstream; closes the lying-doc gap.
- **`int-bits` value changes 64 → 63** → audit stdlib (`imath`, anywhere
  that uses `int-bits`) and examples; one-shot sweep.
- **Shift overflow becomes defined** → `test-bitwise.seq` updates; any
  user code relying on the silent-truncate quirk would change behavior
  (we believe none exists; CHANGELOG should call it out).
- **`int->bits` already exists** → once shift overflow returns 0, the
  non-negative-only restriction in `imath.seq` could be relaxed (negative
  inputs would terminate at width 63 with all-ones-prefix output). Noted
  as a possible follow-up; not in scope here.

## Checkpoints

- `grep "9223372036854775807" docs/language-guide.md` returns nothing
  (the wrong example is gone).
- `test-bitwise.seq` has at least one assertion per shift op against a
  negative operand.
- After piece 3: `int-bits` returns 63; `STDLIB_REFERENCE.md` and
  `language-guide.md` agree.
- After piece 4: `-1 1 shr` returns `0` (or whatever sentinel is chosen,
  documented), not `-1`. A test pins this.
- `int->bits` of a negative still terminates after piece 4 — even if
  `imath.seq` keeps the non-negative contract, the runtime no longer
  loops forever, so a defensive bound there can be removed.
