# Seq Language Grammar

This document provides a formal EBNF grammar specification for the Seq
programming language.

## Notation

- `|` - alternation
- `[ ]` - optional (0 or 1)
- `{ }` - repetition (0 or more)
- `( )` - grouping
- `"..."` - literal terminal
- `UPPERCASE` - lexical tokens
- `lowercase` - grammar rules

---

## Grammar

### Top-Level Structure

```ebnf
program         = { include | union_def | word_def } ;

include         = "include" include_path ;
include_path    = "std" ":" IDENT
                | "ffi" ":" IDENT
                | STRING ;
```

### Union Types (Algebraic Data Types)

```ebnf
union_def       = "union" UPPER_IDENT "{" { union_variant } "}" ;
union_variant   = UPPER_IDENT [ "{" field_list "}" ] ;
field_list      = [ field { "," field } [ "," ] ] ;
field           = IDENT ":" type_name ;
```

### Word Definitions

```ebnf
word_def        = ":" IDENT [ stack_effect ] { statement } ";" ;

stack_effect    = "(" type_list "--" type_list [ "|" effect_annotation { effect_annotation } ] ")" ;
effect_annotation = "Yield" type ;
type_list       = [ row_var ] { type } ;
row_var         = ".." ROW_VAR_NAME ;

type            = base_type
                | type_var
                | quotation_type
                | closure_type ;

base_type       = "Int" | "Float" | "Bool" | "String" ;
type_var        = UPPER_IDENT ;
quotation_type  = "[" type_list "--" type_list "]" ;
closure_type    = "Closure" "[" type_list "--" type_list "]" ;
```

`type_var` must not be the literal token `Quotation`: the parser rejects
it explicitly with a hint pointing at the `[ .. -- .. ]` syntax. The
name `Closure` is also reserved — it's handled as the start of
`closure_type`, not as a type variable.

### Statements

```ebnf
statement       = literal
                | word_call
                | quotation
                | if_stmt
                | match_stmt ;

literal         = INT_LITERAL
                | FLOAT_LITERAL
                | BOOL_LITERAL
                | STRING
                | SYMBOL_LITERAL ;

word_call       = IDENT ;

quotation       = "[" { statement } "]" ;

if_stmt         = "if" { statement } ( "then" | "else" { statement } "then" ) ;

match_stmt      = "match" { match_arm } "end" ;
match_arm       = pattern "->" { statement } ;
pattern         = UPPER_IDENT [ "{" { BINDING } "}" ] ;
BINDING         = ">" IDENT ;
```

`BINDING` is a single lexical token: `>` and the field name must not be
separated by whitespace. `>value` is a binding; `> value` is two
separate tokens (the word calls `>` and `value`) and the parser reports
an error asking for the `>`-prefix form.

---

## Lexical Grammar

### Identifiers

```ebnf
IDENT           = IDENT_START { IDENT_CHAR } ;
IDENT_START     = LETTER | "_" | "-" | "." | ">" | "<" | "=" | "?" | "!" | "+" | "*" | "/" | "%" ;
IDENT_CHAR      = IDENT_START | DIGIT ;

UPPER_IDENT     = UPPER_LETTER { IDENT_CHAR } ;
LOWER_IDENT     = LOWER_LETTER { IDENT_CHAR } ;

ROW_VAR_NAME    = LOWER_LETTER { LETTER | DIGIT | "_" } ;

LETTER          = UPPER_LETTER | LOWER_LETTER ;
UPPER_LETTER    = "A" | "B" | ... | "Z" ;
LOWER_LETTER    = "a" | "b" | ... | "z" ;
DIGIT           = "0" | "1" | ... | "9" ;
```

Row-variable names (`..rest`) use the stricter `ROW_VAR_NAME` rule: they
must start with a lowercase letter and contain only letters, digits, and
underscores. The broader `IDENT` punctuation characters (`- . > < = ? !
+ * / %`) are rejected. The names `Int`, `Bool`, `String` are reserved
even though they're already excluded by the lowercase-start rule (the
parser emits a dedicated error if you try to use them).

### Literals

```ebnf
INT_LITERAL     = DECIMAL_INT | HEX_INT | BINARY_INT ;
DECIMAL_INT     = [ "-" ] DIGIT { DIGIT } ;
HEX_INT         = "0" ( "x" | "X" ) HEX_DIGIT { HEX_DIGIT } ;
BINARY_INT      = "0" ( "b" | "B" ) BINARY_DIGIT { BINARY_DIGIT } ;

HEX_DIGIT       = DIGIT | "a" | "b" | "c" | "d" | "e" | "f"
                        | "A" | "B" | "C" | "D" | "E" | "F" ;
BINARY_DIGIT    = "0" | "1" ;

FLOAT_LITERAL   = [ "-" ] ( DIGIT { DIGIT } "." { DIGIT } [ EXPONENT ]
                          | DIGIT { DIGIT } EXPONENT
                          | "." DIGIT { DIGIT } [ EXPONENT ] ) ;
EXPONENT        = ( "e" | "E" ) [ "+" | "-" ] DIGIT { DIGIT } ;

BOOL_LITERAL    = "true" | "false" ;

SYMBOL_LITERAL  = ":" SYMBOL_NAME ;
SYMBOL_NAME     = LETTER { LETTER | DIGIT | "-" | "_" | "." | "?" | "!" } ;

(* `:` is a single-character delimiter token; whitespace after it is not
   significant. Disambiguation between `word_def` and `SYMBOL_LITERAL` is
   context-driven: a `:` at the top level starts a `word_def`, and a `:`
   inside a word body (wherever a `statement` is expected) starts a
   `SYMBOL_LITERAL`. *)

STRING          = '"' { STRING_CHAR | ESCAPE_SEQ } '"' ;
STRING_CHAR     = any character except '"' or '\' ;
ESCAPE_SEQ      = '\' ( '"' | '\' | 'n' | 'r' | 't' )
                | '\' 'x' HEX_DIGIT HEX_DIGIT ;
```

The `\xNN` escape produces the Unicode code point `U+00NN`. For `NN` in
`00..7F` this is a single ASCII byte (common use: `\x1b` for ANSI
terminal escape sequences). For `NN` in `80..FF` the code point falls
in the Latin-1 Supplement block (`U+0080..U+00FF`) and the resulting
character is encoded as multi-byte UTF-8.

### Comments and Whitespace

```ebnf
COMMENT         = "#" { any character except newline } NEWLINE ;
SHEBANG         = "#!" { any character except newline } NEWLINE ;
WHITESPACE      = SPACE | TAB | NEWLINE ;
```

A `SHEBANG` line (typically `#!/usr/bin/env seqc`) is accepted anywhere a
`COMMENT` is, so scripts can be executed directly from the shell. The
parser treats it as an ordinary comment.

Comments matching the form `# seq:allow(lint-id)` are collected as lint
allowances for the word definition that follows them. The text inside
the parentheses is the lint rule id; multiple `seq:allow` comments
before a word stack additively.

---

## Semantic Notes

### Row Polymorphism

All stack effects are implicitly row-polymorphic. When no explicit row variable is given, an implicit `..rest` is assumed:

```seq
# These are equivalent:
: dup ( T -- T T ) ... ;
: dup ( ..rest T -- ..rest T T ) ... ;
```

This means `( -- )` preserves the stack (it's `( ..rest -- ..rest )`), not that it requires an empty stack.

### Naming Conventions

| Delimiter | Usage | Example |
|-----------|-------|---------|
| `.` (dot) | Module/namespace prefix | `io.write-line`, `net.tcp.listen` |
| `-` (hyphen) | Compound words | `home-dir`, `write-line` |
| `->` (arrow) | Type conversions | `int->string`, `float->int` |
| `?` (question) | Predicates | `list.empty?`, `map.has?` |

For each `union` definition, the compiler auto-generates helper words
by convention. Given `union Shape { Circle { radius: Int } … }`:

| Generated word | Shape | Example |
|----------------|-------|---------|
| `Make-<Variant>` | constructor | `5 Make-Circle` |
| `is-<Variant>?` | predicate | `shape is-Circle?` |
| `<Variant>-<field>` | field accessor | `circle Circle-radius` |

These are ordinary `word_call`s at the grammar level; they're listed
here so readers can predict the generated names.

### Reserved Words

The following are reserved and cannot be used as word names:

- Control flow: `if`, `else`, `then`, `match`, `end`
- Definitions: `union`, `include`
- Literals: `true`, `false`

### Operator Precedence

Seq has no operator precedence - all tokens are either literals or word calls. Evaluation is strictly left-to-right with stack-based semantics.

### Quotations vs Closures

A `quotation` (the surface syntax `[ … ]`) has two possible types:

- `quotation_type` — if the body consumes only values pushed inside the
  quotation itself (plus an implicit row variable).
- `closure_type` — if the body references values from the enclosing
  stack. The compiler captures those values into an environment at the
  point the quotation is produced; the result is a `Closure[ … ]` at
  the type level.

There is no dedicated syntax for a closure — the parser always builds a
quotation literal, and the type checker decides whether the result is a
`quotation_type` or a `closure_type` based on what the body references.

### Arithmetic Sugar

The tokens `+`, `-`, `*`, `/`, `%`, `=`, `<`, `>`, `<=`, `>=`, and `<>` are
ordinary identifiers at the grammar level but are resolved by the compiler
to their typed counterparts based on the inferred stack types. For example:

```seq
3 4 +        # resolves to `i.+` — both operands are Int
3.0 4.0 +    # resolves to `f.+` — both operands are Float
```

This is a compile-time rewrite, not dynamic dispatch: if the types can't
be inferred unambiguously the program fails to type-check. Writing the
explicit form (`i.+`, `f.<`, etc.) is always valid and suppresses the
sugar resolution.

Sugar resolves only when the operand types are visible on the
typechecker's stack at the use site. **Inside a quotation body the body
is typed against its own fresh effect, so its stack is empty from the
resolver's perspective and sugar cannot resolve.** Use the typed form
inside quotations:

```seq
3 4 [ + ] call         # error: + can't resolve, operands not in scope
3 4 [ i.+ ] call       # idiomatic — works regardless of caller context
```

The typed form (`i.+`, `f.+`, `string.concat`, …) is the always-works
idiom; sugar is a top-level convenience that's nice for short
expressions but should be expanded when writing words intended to be
passed to combinators like `dip`, `keep`, `bi`, `times`, or
`each-integer`.

---

## Examples

### Complete Program

```seq
include std:json

union Result {
  Ok { value: Int }
  Error { message: String }
}

: safe-divide ( Int Int -- Result )
  dup 0 i.= if
    drop drop "Division by zero" Make-Error
  else
    i.divide drop Make-Ok
  then
;

: main ( -- )
  10 2 safe-divide
  match
    Ok { >value } -> value int->string io.write-line
    Error { >message } -> message io.write-line
  end
;
```

### Stack Effects

```seq
# Simple transformation
: double ( Int -- Int ) 2 i.* ;

# Multiple inputs/outputs
: divmod ( Int Int -- Int Int ) over over i./ rot rot i.% ;

# Row-polymorphic (preserves rest of stack)
: swap ( ..a T U -- ..a U T ) ... ;

# Quotation type
: apply-twice ( Int [Int -- Int] -- Int ) dup rot swap call swap call ;

# Closure type
: make-adder ( Int -- Closure[Int -- Int] ) [ i.+ ] ;
```
