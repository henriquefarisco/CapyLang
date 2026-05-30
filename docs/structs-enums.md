# S6.3 - struct / enum at runtime + match aggregate lowering (design)

Status: **draft / design.** Specifies slice **S6.3** of `docs/roadmap.md`.
Not implemented; opcodes and the `Value` shape below enter the frozen
contract (`docs/bytecode-v0.md`, `docs/compatibility.md`) only when
implemented and `make rust-validate` passes. Builds directly on the S6.2
aggregate machinery (`docs/aggregates.md`).

S6.3 makes `struct` and `enum` usable at runtime and completes the
`match` lowering for the `TupleStruct` / `Struct` / `Path` patterns that
parse today (S2.2b) but the emitter still rejects with `E0010`. Together
with S6.2 arrays this is the data model the Snake benchmark needs
(`Point`, `Direction`, game state).

## What already exists (frontend)

The AST and parser already model the full surface:

- `Item::Struct(StructItem { name, fields })`,
  `Item::Enum(EnumItem { name, variants })`,
  `Variant { name, body }` with
  `VariantBody::{ Unit, Tuple(Vec<Type>), Struct(Vec<StructField>) }`.
- Patterns `Pattern::TupleStruct { path, elems }`,
  `Pattern::Struct { path, fields, has_rest }`, `Pattern::Path`.

What is **missing** in the frontend:

- A **struct-literal expression** `Point { x: 1, y: 2 }` - there is no
  `Expr` variant for it (see "Parser" below; it carries a grammar
  ambiguity).
- Enum construction reuses existing nodes: a unit variant `Color::Red` is
  an `Expr::Path`; a tuple variant `Some(5)` is an `Expr::Call` whose
  callee is a path.

What the emitter rejects today (the removal targets):

- `Item::Struct` / `Item::Enum` / `Item::TypeAlias` -> `E0002`
  `UnsupportedItem` (`emit.rs` ~150-164).
- `Expr::Field` (`p.x`) -> `E0001` `UnsupportedExpr` (~630).
- `Pattern::TupleStruct` / `Struct` / `Path` / `Rest` -> `E0010`
  `UnsupportedFeature` (~1338-1358).

## Runtime value model

Add one tagged-aggregate variant (reusing S6.2 reference semantics):

```rust
pub enum Value {
    // ... None / Bool / Int / Float / Str / Array (S6.2) ...
    Aggregate {
        tag: u32,
        fields: Rc<RefCell<Vec<Value>>>,   // NEW (S6.3)
    },
}
```

- A **struct** value is an aggregate whose `tag` is the struct type's tag
  and whose `fields` are the field values in **declaration order**.
- An **enum** value is an aggregate whose `tag` is the *variant's* tag;
  `fields` is the variant payload (empty for unit, positional for tuple,
  declaration-ordered for struct variants).
- Structs and enum variants therefore share one representation and one
  flat tag space. The VM treats `tag` opaquely - it only stores and
  compares it; all naming lives in the emitter.
- Reference semantics, opacity, `forbid(unsafe_code)` and the
  derived-`PartialEq` / in-language-comparison-traps rules are identical
  to `Value::Array` (see `docs/aggregates.md`).

## Tag and layout registry (emitter, compile-time only)

A **pre-pass** over `Source.items` (before lowering function bodies)
builds, deterministically in declaration order:

- `type_tag: name -> u32` for each `struct`;
- `variant_tag: (enum_name, variant_name) -> u32` for each enum variant
  (and the reverse for `Path` resolution);
- `layout: tag -> Vec<field_name>` for structs and struct-variants;
  `arity: tag -> u32` for tuple-variants.

Tags are a single counter shared by structs and variants. They are
**emitter-internal**: baked into `MakeAggregate` / compared after
`GetTag`. **No new bytecode section is required** - the metadata is
compile-time only and never serialised. A malformed module with
mismatched tags simply fails to match (fail-closed), never crashes.

### No type checker required

S6.3 deliberately avoids needing a type system:

- **Construction** always names the type/variant syntactically
  (`Point { .. }`, `Color::Red`, `Some(x)`), so the emitter resolves the
  tag and field order from the registry.
- **Destructuring** is done by `match` patterns, which also name the type
  via their `path`, so field-name -> index resolution uses the declared
  layout.
- **Field-access expressions `p.x` stay deferred**: resolving `.x` to an
  index needs `p`'s type, which needs the (not-yet-built) type checker.
  S6.3 keeps the `E0001` rejection for `Expr::Field` but improves the
  message to point users at `match` destructuring. (Field access lands
  with the type checker, or via a later dynamic by-name opcode.)

## Opcodes (proposed, additive; 0x64-0x66 in the aggregate block)

| Byte | Mnemonic        | Immediate | Stack effect                                    |
|-----:|-----------------|-----------|-------------------------------------------------|
| 0x64 | `make_aggregate`| `U32U32`  | `v0..vN-1 -> agg` (`tag`, `field_count = N`)    |
| 0x65 | `get_field`     | `U32`     | `agg -> agg.fields[index]`                       |
| 0x66 | `get_tag`       | -         | `agg -> tag` (`Int`)                             |

`make_aggregate (tag, n)` pops the top `n` values (first-pushed = field
0) and pushes the aggregate. `get_tag` pushes the tag as an `Int` so the
existing `Eq` + `JumpIfFalse` lowering can branch on it. `get_field`
clones the field out.

### Verifier stack effects

| Instruction          | `required_inputs` | `produced_outputs` |
|----------------------|-------------------|--------------------|
| `MakeAggregate(_, n)`| `n`               | `1`                |
| `GetField(_)`        | `1`               | `1`                |
| `GetTag`             | `1`               | `1`                |

### VM semantics

- `MakeAggregate(tag, n)`: pop `n`, push
  `Value::Aggregate { tag, fields: Rc::new(RefCell::new(vec)) }`.
- `GetField(i)`: pop aggregate (else `V0005`); if `i >= fields.len()`
  trap `V0018 FIELD_OUT_OF_BOUNDS`; push `fields[i].clone()`.
- `GetTag`: pop aggregate (else `V0005`), push `Int(tag)`.

### New error code

`V0018 FIELD_OUT_OF_BOUNDS` (pc + index + len). Additive to the VM
catalogue; mirror into `docs/compatibility.md`. (Kept distinct from the
array-specific `V0017` so messages stay precise.)

## Lowering

### Declarations

`Item::Struct` / `Item::Enum` emit **no code**; they only populate the
registry in the pre-pass. `Item::TypeAlias` stays a no-op (or a thin
registry alias). Replace the `E0002` rejections accordingly.

### Construction

- Unit variant (`Color::Red`, an `Expr::Path`): `MakeAggregate(tag, 0)`.
- Tuple variant (`Some(5)`, an `Expr::Call` with a path callee): emit
  args, `MakeAggregate(tag, argc)`. `emit_call` must consult the registry
  first: a path that names a variant lowers to `MakeAggregate`, otherwise
  it stays a function/host call.
- Struct literal (`Point { y: 2, x: 1 }`): resolve the struct's declared
  field order from the registry, **emit field initialisers in
  declaration order** (reordering the literal's fields), then
  `MakeAggregate(tag, field_count)`.

### Match (the payoff)

The existing match lowering evaluates the scrutinee once into a temp
local and tests arms in order. Add three pattern arms (replacing the
`E0010` rejections):

- `Pattern::Path` (unit variant): `LoadLocal scrut; GetTag;
  LoadConst Int(tag); Eq; JumpIfFalse next_arm`.
- `Pattern::TupleStruct { path, elems }`: tag test as above; then for each
  `elems[i]`: `LoadLocal scrut; GetField i;` and recurse into the
  sub-pattern (an `Ident` sub-pattern lowers to `StoreLocal binding`; a
  literal sub-pattern to a nested test; nested aggregates recurse).
- `Pattern::Struct { path, fields, has_rest }`: tag test; then for each
  field, resolve `field_name -> index` from the layout, `LoadLocal scrut;
  GetField index;` and recurse into the field's sub-pattern. `has_rest`
  (`..`) simply skips unbound fields.

Binding sub-patterns reuses the S2.2b machinery; only the
"extract a component" step is new (`GetField` instead of the scrutinee
itself). Guard (`if`) handling is unchanged.

## Parser: the struct-literal ambiguity

`Point { x: 1 }` collides with block syntax: in `if cond { ... }` the
`{` opens a block, not a struct literal. Adopt Rust's rule - struct
literals are **not** parsed in a "no-struct-literal" context (the
condition of `if` / `while` / `match`, and the iterable of `for`).

- Add `Expr::StructLit { path: Vec<Ident>, fields: Vec<StructLitField>,
  has_rest: bool, span }` and `StructLitField { name, value, span }`
  (shorthand `Point { x }` records `value = Ident(x)`).
- Thread a `no_struct_literal: bool` flag through expression parsing.
  Set it true when parsing those head positions; a path followed by `{`
  is then a path expression + block boundary, not a literal.
- Only attempt struct-literal parsing when an identifier/path primary is
  immediately followed by `{` and the flag is clear.

This is the single trickiest part of S6.3 and the reason for staging.

## Suggested staging (one validatable slice each)

- **S6.3a** - value model + the three opcodes + verifier + VM + a new
  `V0018`. Testable by building modules directly (no frontend), like the
  S6.2 bytecode tests. Lowest risk.
- **S6.3b** - enum construction (unit `Path`, tuple `Call`) + `match`
  `Path` / `TupleStruct` patterns. Enables `Option` / `Result`-style
  enums end-to-end with **no parser-ambiguity work**. High value.
- **S6.3c** - `struct` declarations + struct-literal expressions (with the
  `no_struct_literal` context) + `Struct` patterns (by-name binding).

## Exhaustive-match checklist (so the build stays green)

`Value::Aggregate` touches the same sites as `Value::Array` did:

- `capy-vm/value.rs`: `Value::type_name` (add `"struct"` or `"aggregate"`).
- `capy-vm/execute.rs`: the new `MakeAggregate` / `GetField` / `GetTag`
  arms in the main `match`; arithmetic / comparison ops keep their
  catch-alls (aggregates trap with `TYPE_MISMATCH`).
- `capyc/main.rs`: `format_value` (render aggregates, e.g.
  `#<tag>(f0, f1)` - choose a deterministic form).
- `capy-bytecode`: `Opcode` (enum / `from_byte` / `mnemonic` /
  `immediate` for `MakeAggregate` = `U32U32`, `GetField` = `U32`);
  `Instruction` (enum / `opcode` / `encode_into` / `decode` /
  `disassemble_text`); `required_inputs` / `produced_outputs`; round-trip
  test arrays.
- `capy-ast`: `Expr::StructLit` -> `span()` and `dump` (S6.3c only);
  `is_block_like` unaffected.
- `capy-vm/error.rs`: `V0018` (variant, `code()`, `pc()`, `Display`,
  the every-variant test).

## Acceptance criteria

- `make rust-validate` green per sub-slice.
- Round-trip + disassembly of the new opcodes; verifier stack effects.
- VM end-to-end (S6.3b): construct `Some(5)`; `match` it to bind `x = 5`;
  `match Color::Red` selects the right arm; an out-of-range `get_field`
  and a `get_tag` on a non-aggregate trap with `V0018` / `V0005`.
- VM end-to-end (S6.3c): `Point { x: 1, y: 2 }` then `match p { Point { x,
  y } => x + y }` -> `3`, including reordered literal fields.
- Docs updated: `docs/bytecode-v0.md`, `docs/compatibility.md`,
  `docs/grammar.ebnf`, `docs/roadmap.md`, `CHANGELOG.md`.

## Follow-on

- Field-access expressions `p.x` (need the type checker, or a dynamic
  by-name field opcode).
- `match` exhaustiveness checking over enum variants (a type-checker
  concern).
- Tuples as first-class values (a structural aggregate with a reserved
  tag) if the grammar adds tuple expressions.
