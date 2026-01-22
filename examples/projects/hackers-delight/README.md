# Hacker's Delight Examples

Bit manipulation puzzles inspired by the classic techniques in low-level programming.

## Files

| File | Topic |
|------|-------|
| `01-rightmost-bits.seq` | Rightmost bit manipulation (turn off, isolate, propagate) |
| `02-power-of-two.seq` | Power of 2 detection, next power, log2 |
| `03-counting-bits.seq` | Popcount algorithms, parity, leading/trailing zeros |
| `04-branchless.seq` | Branchless abs, sign, min, max |
| `05-swap-reverse.seq` | XOR swap, bit reversal, bit set/clear/toggle |

## Running

```bash
seqc examples/hackers-delight/01-rightmost-bits.seq -o /tmp/demo && /tmp/demo
```

## Bitwise Operations Used

These examples use Seq's bitwise operations:

- `band` - bitwise AND
- `bor` - bitwise OR
- `bxor` - bitwise XOR
- `bnot` - bitwise NOT
- `shl` - shift left
- `shr` - logical shift right
- `popcount` - count 1-bits
- `clz` - count leading zeros
- `ctz` - count trailing zeros
- `int-bits` - bit width (64)

## Numeric Literals

Seq supports hex and binary literals for bit manipulation:

```seq
0xFF        # hex: 255
0b10101010  # binary: 170
```
