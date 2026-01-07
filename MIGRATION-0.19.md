# Migration Guide: 0.18.x to 0.19.0

## Channel Type

Seq 0.19 introduces a `Channel` type. Previously, channels used `Int` in type signatures.

### Update Function Signatures

Change `Int` to `Channel` for channel parameters:

```seq
# Before
: worker ( Int Int -- )
  swap chan.receive drop
  swap chan.send drop
;

# After
: worker ( Channel Channel Int -- )
  swap chan.receive drop
  swap chan.send drop
;
```

### Operation Signatures

| Operation | Old | New |
|-----------|-----|-----|
| `chan.make` | `( -- Int )` | `( -- Channel )` |
| `chan.send` | `( T Int -- Bool )` | `( T Channel -- Bool )` |
| `chan.receive` | `( Int -- T Bool )` | `( Channel -- T Bool )` |
| `chan.close` | `( Int -- )` | `( Channel -- )` |

### Finding Affected Code

The compiler reports errors like:

```
Error: chan.send: stack type mismatch. Expected (..a T Channel), got (..rest Int Int):
Type mismatch: cannot unify Channel with Int
```

Search for channel-using functions and update their signatures from `Int` to `Channel`.
