# Seq - Concatenative Language

A concatenative, stack-based programming language with linear types, built on a solid foundation.

## What's Different from cem2?

Seq separates **Values** (what the language talks about) from **StackNodes** (implementation details):

- **Value**: Pure data (Int, Bool, String, Variant)
- **StackNode**: Container with value + next pointer
- **Variant**: Fields stored in arrays, NOT linked via next pointers

This clean separation ensures that stack shuffling operations (`rot`, `swap`, etc.) never corrupt variant structures.

## Project Status

🚧 **Phase 0: Foundation** - Building core types and basic operations

See `docs/ROADMAP.md` for the full development plan.

## Philosophy

**Foundation First:** Get the concatenative core bulletproof before adding advanced features.

**No Compromises:** If something doesn't feel clean, we stop and redesign.

**Learn from cem2:** cem2 taught us what happens when you conflate StackCell with Value. Seq does it right from the start.

## Building

```bash
cargo build --release
cargo test
```

## Documentation

- `docs/ROADMAP.md` - Development phases and milestones
- `docs/CLEAN_CONCATENATIVE_DESIGN.md` - Core design principles
- `docs/CELL_VS_VALUE_DESIGN.md` - Why we separate Value from StackNode
- `docs/CONCATENATIVE_CORE_INVARIANTS.md` - Invariants that must hold

## License

See LICENSE file.
