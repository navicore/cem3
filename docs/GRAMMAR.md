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
word_def        = ":" IDENT stack_effect { statement } ";" ;

stack_effect    = "(" type_list "--" type_list ")" ;
type_list       = [ row_var ] { type } ;
row_var         = ".." LOWER_IDENT ;

type            = base_type
                | type_var
                | quotation_type
                | closure_type ;

base_type       = "Int" | "Float" | "Bool" | "String" ;
type_var        = UPPER_IDENT ;
quotation_type  = "[" type_list "--" type_list "]" ;
closure_type    = "Closure" "[" type_list "--" type_list "]" ;
```

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
                | STRING ;

word_call       = IDENT ;

quotation       = "[" { statement } "]" ;

if_stmt         = "if" { statement } ( "then" | "else" { statement } "then" ) ;

match_stmt      = "match" { match_arm } "end" ;
match_arm       = pattern "->" { statement } ;
pattern         = UPPER_IDENT [ "{" { binding } "}" ] ;
binding         = ">" IDENT ;
```

---

## Lexical Grammar

### Identifiers

```ebnf
IDENT           = IDENT_START { IDENT_CHAR } ;
IDENT_START     = LETTER | "_" | "-" | "." | ">" | "<" | "=" | "?" | "!" | "+" | "*" | "/" ;
IDENT_CHAR      = IDENT_START | DIGIT ;

UPPER_IDENT     = UPPER_LETTER { IDENT_CHAR } ;
LOWER_IDENT     = LOWER_LETTER { IDENT_CHAR } ;

LETTER          = UPPER_LETTER | LOWER_LETTER ;
UPPER_LETTER    = "A" | "B" | ... | "Z" ;
LOWER_LETTER    = "a" | "b" | ... | "z" ;
DIGIT           = "0" | "1" | ... | "9" ;
```

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

STRING          = '"' { STRING_CHAR | ESCAPE_SEQ } '"' ;
STRING_CHAR     = any character except '"' or '\' ;
ESCAPE_SEQ      = '\' ( '"' | '\' | 'n' | 'r' | 't' ) ;
```

### Comments and Whitespace

```ebnf
COMMENT         = "#" { any character except newline } NEWLINE ;
WHITESPACE      = SPACE | TAB | NEWLINE ;
```

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
| `.` (dot) | Module/namespace prefix | `io.write-line`, `tcp.listen` |
| `-` (hyphen) | Compound words | `home-dir`, `write-line` |
| `->` (arrow) | Type conversions | `int->string`, `float->int` |
| `?` (question) | Predicates | `path-exists?`, `empty?` |

### Reserved Words

The following are reserved and cannot be used as word names:

- Control flow: `if`, `else`, `then`, `match`, `end`
- Definitions: `union`, `include`
- Literals: `true`, `false`

### Operator Precedence

Seq has no operator precedence - all tokens are either literals or word calls. Evaluation is strictly left-to-right with stack-based semantics.

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
    i.divide Make-Ok
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
