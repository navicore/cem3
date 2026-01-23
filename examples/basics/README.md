# Basics

Getting started with Seq - the simplest programs to verify your setup.

## hello-world.seq

The canonical first program:

```seq
: main ( -- Int ) "Hello, World!" io.write-line 0 ;
```

## cond.seq

Demonstrates the `cond` combinator for multi-way branching - a cleaner alternative to nested if/else.
