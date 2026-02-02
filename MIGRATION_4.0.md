# Migration Guide: Seq 3.x to 4.0

This guide covers the breaking changes in Seq 4.0 and how to update your code.

## Overview

Seq 4.0 introduces **compile-time safety for union types** (RFC #345). Union
types now have proper nominal typing, and the compiler auto-generates type-safe
accessor words. This catches type errors at compile time that would previously
cause runtime crashes.

## What's New

### Auto-Generated Words

For each union variant, the compiler now generates:

```seq
union Message {
  Get { response_chan: Int }
  Set { key: String, value: Int }
}

# Compiler generates:

# Constructors (already existed)
: Make-Get ( Int -- Message ) ...
: Make-Set ( String Int -- Message ) ...

# NEW: Type predicates
: is-Get? ( Message -- Bool ) ...
: is-Set? ( Message -- Bool ) ...

# NEW: Field accessors
: Get-response_chan ( Message -- Int ) ...
: Set-key ( Message -- String ) ...
: Set-value ( Message -- Int ) ...
```

### Type-Safe Constructors

Constructors now return the specific union type instead of generic `V`:

```seq
# Before (3.x): Make-Get returned generic Variant
# After (4.0): Make-Get returns Message

42 Make-Get  # Returns Message, not Variant
```

## Breaking Changes

### 1. Name Collisions with Generated Words

**Problem**: If you defined words with names matching the new auto-generated
pattern, you'll get a collision error.

```seq
# This will now fail - conflicts with auto-generated accessor
: Get-response_chan ( Message -- Int )
  0 variant.field-at ;
```

**Fix**: Remove your manual definitions and use the auto-generated words, or
rename your words.

```seq
# Option 1: Just delete your definition - use the auto-generated one

# Option 2: Rename if you need different behavior
: my-get-response-chan ( Message -- Int )
  0 variant.field-at ;
```

### 2. Stricter Union Type Checking

**Problem**: Code that mixed different union types now fails to compile.

```seq
union Sexpr { SNum { value: Int } }
union SexprList { SNil | SCons { head: Sexpr, tail: SexprList } }

# This worked in 3.x but fails in 4.0:
: broken ( Sexpr SexprList -- SexprList )
  Make-SCons ;  # Error: Make-SCons expects (Sexpr SexprList), got correct types but...
```

Actually, the above would work. The real issue is when functions declare generic
`Variant` but receive typed unions:

```seq
# 3.x: This worked because Variant unified with anything
: process ( Variant Variant -- Variant )
  ...

# 4.0: If you call this with (Sexpr SexprList), both must unify
# to the SAME type, which fails because Sexpr != SexprList
```

**Fix**: Use distinct type variable names when parameters can be different types:

```seq
# Before (broken in 4.0)
: process ( Variant Variant -- Variant )

# After (works)
: process ( A B -- C )
```

### 3. Type Annotations with Repeated Names

**Problem**: In Seq's type system, repeated type variable names must unify to
the same type.

```seq
# This means "three values of the SAME type"
: foo ( T T T -- T )

# If you call foo with (Int String Bool), it fails because
# Int, String, and Bool can't all be the same type
```

**Fix**: Use different names for different types:

```seq
# This means "three values of potentially DIFFERENT types"
: foo ( A B C -- D )
```

### 4. Dynamic Code Using variant.field-at

**Problem**: If you were using `variant.field-at` on typed unions, the type
checker now tracks the union type more precisely.

**Fix**: This still works - `variant.field-at` is the escape hatch for dynamic access:

```seq
# This is fine - variant.field-at works on any variant
: dynamic-access ( Message -- Int )
  0 variant.field-at ;
```

## Migration Steps

### Step 1: Remove Manual Accessors and Predicates

Search your code for patterns like:

```seq
: VariantName-fieldname ( ... ) ... variant.field-at ;
: is-VariantName? ( ... ) variant.tag :VariantName symbol.= ;
```

Delete these - the compiler generates them automatically now.

### Step 2: Fix Type Signature Collisions

If you get errors about repeated type variables, rename them:

```seq
# Change this:
: my-func ( Variant Variant Variant -- Variant )

# To this (use descriptive names):
: my-func ( Env List Head -- Result )

# Or use single letters:
: my-func ( A B C -- D )
```

### Step 3: Use Generated Accessors

Replace manual field access with generated accessors:

```seq
# Before
: get-name ( Person -- String )
  0 variant.field-at ;

# After (assuming: union Person { Person { name: String, age: Int } })
# Just use: Person-name
my-person Person-name  # Returns the name field
```

### Step 4: Use Generated Predicates

Replace manual tag checks:

```seq
# Before
: is-some? ( Option -- Bool )
  variant.tag :Some symbol.= ;

# After
my-option is-Some?
```

## Escape Hatches

For truly dynamic code (FFI interop, metaprogramming), you can still use
low-level operations:

```seq
# Create variant with dynamic tag
:MyTag variant.make-2

# Access fields dynamically
0 variant.field-at

# Check tag dynamically
variant.tag :Expected symbol.=
```

These bypass the type-safe accessors when you need maximum flexibility.

## Example: Updating a Lisp Interpreter

Here's a real example from updating the lisp project:

### Before (3.x)

```seq
# Environment used SexprList for storage
: env-extend ( String Variant Variant -- Variant )
  rot rot make-binding swap scons ;  # scons expects (Sexpr SexprList)
```

### After (4.0)

```seq
# Use separate environment list operations
: env-cons ( Binding EnvList -- EnvList2 )
  :EnvCons variant.make-2 ;

: env-extend ( Name Val EnvList -- EnvList2 )
  rot rot make-binding swap env-cons ;
```

The key changes:
1. Separate env list from Sexpr list (they're different types now)
2. Use distinct type variable names (`Name Val EnvList` instead of `Variant Variant Variant`)

## Benefits

After migration, you get:

1. **Compile-time safety**: Wrong union type errors caught before runtime
2. **Better documentation**: Type signatures show actual types, not generic `Variant`
3. **Less boilerplate**: No need to write accessors and predicates manually
4. **IDE support**: LSP can provide better completions for generated words

## Questions?

If you encounter issues not covered here, please open an issue at:
https://github.com/navicore/patch-seq/issues
