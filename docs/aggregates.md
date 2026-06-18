# S6.2 - aggregate value model (DRAFT design)

Status: **implemented (S6.2), pending external validation.** This
document specifies the aggregate value model (arrays). The opcodes and
`Value` shape below have been implemented and promoted into
`docs/bytecode-v0.md` and `docs/compatibility.md`; the change still needs
`make rust-validate` on a build machine before it is committed. Tuples /
structs / enum payloads remain a follow-on (S6.3).

S6.2 adds **arrays** - the smallest, highest-leverage aggregate. Tuples,
structs and enum payloads build on the same machinery and are deferred to
**S6.3** (which also completes the `match` tuple-struct / struct / path
lowering). Arrays alone unblock the snake body and the game grid.

## Why arrays first

The `Value` model today is scalar only (`None` / `Bool` / `Int` / `Float`
/ `Str`). Nothing can hold a variable-length sequence, so the snake body,
a grid row, or any list is inexpressible. Arrays are the minimal addition
that removes that wall and keep the determinism / fail-closed contract.

## Value model

Extend `capy_vm::Value`:

```rust
pub enum Value {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Rc<RefCell<Vec<Value>>>),   // NEW
}
```

Decision - **reference semantics** via `Rc<RefCell<Vec<Value>>>`:

- A game mutates one array in place across many frames; reference
  semantics (`a` and an alias share one backing store) is what callers
  expect and avoids O(n) copy-on-write per element write.
- It stays **deterministic and single-threaded**; no host pointer is ever
  exposed to bytecode - the array is an opaque handle manipulated only by
  the array opcodes.
- `forbid(unsafe_code)` is preserved (`Rc` / `RefCell` are safe).
- The derived `PartialEq` compares arrays **by value** (element-wise),
  which host-side tests use. In-language comparison is scoped out of
  S6.2: the VM's `Eq` / `Ne` / ordering opcodes trap on arrays with
  `TYPE_MISMATCH` (a structural in-language `==` can be added later
  without a wire change). This keeps the slice small.

Alternative considered - value semantics (`Vec<Value>`, clone on copy):
simpler to reason about but makes in-place mutation O(n) per write and
surprises callers who expect a bound array to be mutable through aliases.
Rejected for the game use-case; revisit only if `Rc` causes trouble.

## Opcodes (proposed, additive within v0)

Allocated in the free `0x60-0x6F` block (today unused), reserved for
"aggregate" operations:

| Byte | Mnemonic     | Immediate | Stack effect                                  |
|-----:|--------------|-----------|-----------------------------------------------|
| 0x60 | `make_array` | `U32` (n) | pops `n` values -> pushes one `Array`         |
| 0x61 | `array_get`  | -         | `arr idx -> arr[idx]`                          |
| 0x62 | `array_set`  | -         | `arr idx val -> arr` (mutates in place)        |
| 0x63 | `array_len`  | -         | `arr -> len(arr)` (`Int`)                      |

`make_array n` pops the top `n` values; the first value pushed becomes
index `0` (source order). With reference semantics `array_set` mutates the
shared backing and pushes the **same** array handle back (so the result is
usable as an expression and as the right-hand input of a store).

These four byte values are additive: adding them does not touch any
existing opcode. The tagged-aggregate ops `make_aggregate` / `get_field`
/ `get_tag` took `0x64-0x66` (S6.3a), and the growable-array ops
`array_push` / `array_pop` took `0x67-0x68` (S6.2b, below); the rest of
`0x60-0x6F` stays reserved for future aggregate ops (e.g. `make_tuple`).

## Growable arrays (S6.2b, additive — implemented, pending validation)

S6.2 ships *fixed-capacity* arrays: `make_array n` allocates exactly `n`
slots and there is no way to change the length afterwards, so "the snake
body" is only expressible if its maximum size is known up front. S6.2b
removes that wall with the two `Vec`-style opcodes:

| Byte | Mnemonic     | Immediate | Stack effect                                  |
|-----:|--------------|-----------|-----------------------------------------------|
| 0x67 | `array_push` | -         | `arr val -> arr` (appends `val`, grows by one) |
| 0x68 | `array_pop`  | -         | `arr -> val` (removes + returns the last elem) |

- `array_push` pops `(arr, val)`, requires `arr` is an `Array` (else
  `V0005 TYPE_MISMATCH`), appends `val` to the shared backing in place and
  pushes the **same** array handle back (reference semantics, mirroring
  `array_set`). The new element lands at index `len-1`.
- `array_pop` pops `arr` (require `Array`), removes the last element and
  pushes it. Popping an empty array is fail-closed with the new
  `V0019 POP_EMPTY_ARRAY` trap (kept distinct from the index-oriented
  `V0017` so the message stays precise).
- Verifier stack effects: `ArrayPush` `required_inputs = 2`,
  `produced_outputs = 1`; `ArrayPop` `required_inputs = 1`,
  `produced_outputs = 1`. Both are ordinary (non-control-flow)
  instructions, so only the two stack-effect tables need new arms.
- Determinism preserved: no length cap beyond the host's memory; push /
  pop are O(1) amortised; no iteration-order ambiguity.

This sub-slice is **bytecode + VM only** (testable by building modules
directly, like S6.3a); the `a.push(x)` / `a.pop()` frontend surface waits
on the method-call / stdlib work (S10).

## Insert / remove (S6.2c, additive — implemented, pending validation)

S6.2b only grows / shrinks at the tail. S6.2c adds **positional** insert /
remove so the body can splice at an arbitrary index, with the two remaining
`Vec`-style opcodes:

| Byte | Mnemonic       | Immediate | Stack effect                                       |
|-----:|----------------|-----------|----------------------------------------------------|
| 0x69 | `array_insert` | -         | `arr idx val -> arr` (inserts at `idx`, grows by one) |
| 0x6A | `array_remove` | -         | `arr idx -> val` (removes + returns the elem at `idx`) |

- `array_insert` pops `(arr, idx, val)`, requires `arr` is an `Array` (else
  `V0005 TYPE_MISMATCH`) and `idx` an `Int`, inserts `val` at `idx` in the
  shared backing in place (elements at `idx..` shift right) and pushes the
  **same** array handle back (reference semantics, mirroring `array_set`).
  The valid range is `0..=len`: `idx == len` appends, exactly like
  `array_push`.
- `array_remove` pops `(arr, idx)` (require `Array` + `Int`), removes the
  element at `idx` (elements at `idx+1..` shift left) and pushes it. The
  valid range is `0..len`.
- Out-of-range is fail-closed with the existing `V0017 INDEX_OUT_OF_BOUNDS`
  (negative index, `idx > len` for insert, or `idx >= len` for remove —
  including any index into an empty array). No new trap is introduced: an
  insert / remove is an index operation, so it shares `array_get` /
  `array_set`'s index trap rather than minting a parallel code.
- Verifier stack effects: `ArrayInsert` `required_inputs = 3`,
  `produced_outputs = 1`; `ArrayRemove` `required_inputs = 2`,
  `produced_outputs = 1`. Both are ordinary (non-control-flow) instructions,
  so only the two stack-effect tables need new arms.
- Determinism preserved: insert / remove are O(n) in the shift distance but
  fully deterministic; no iteration-order ambiguity.

This sub-slice is **bytecode + VM only**; the `a.insert(i, x)` /
`a.remove(i)` frontend surface waits on the method-call / stdlib work (S10).

## Verifier stack effects (`capy-bytecode/src/verify.rs`)

| Instruction  | `required_inputs` | `produced_outputs` |
|--------------|-------------------|--------------------|
| `MakeArray(n)` | `n`             | `1`                |
| `ArrayGet`     | `2`             | `1`                |
| `ArraySet`     | `3`             | `1`                |
| `ArrayLen`     | `1`             | `1`                |

All are ordinary (non-control-flow) instructions, so the verifier's
existing sequential successor handling applies; only the two stack-effect
tables need the new arms.

## VM semantics (`capy-vm/src/execute.rs`)

- `MakeArray(n)`: pop `n` values into a `Vec` (preserving source order),
  push `Value::Array(Rc::new(RefCell::new(vec)))`.
- `ArrayGet`: pop `idx`, pop `arr`. Require `arr` is `Array` and `idx` is
  `Int`, else `V0005 TYPE_MISMATCH`. If `idx < 0 || idx >= len`, trap with
  the new `V0017 INDEX_OUT_OF_BOUNDS`. Else push `arr[idx].clone()`.
- `ArraySet`: pop `val`, `idx`, `arr`. Same type / bounds checks; write
  `arr.borrow_mut()[idx] = val`; push `arr` back.
- `ArrayLen`: pop `arr` (require `Array`), push `Int(len)` (`len` fits
  `i64` for any realistic program; clamp / saturate defensively).
- Determinism: all element access is bounds-checked and fail-closed; no
  iteration order ambiguity; equality is element-wise and recursive.

### New error code

`V0017 INDEX_OUT_OF_BOUNDS` (pc + index + len). Additive to the
`V0001-V0016` catalogue; mirror into `docs/compatibility.md` and the VM
error model.

## Frontend (`capy-ast`, `capy-parser`)

- **AST**: new `Expr::Array { elems: Vec<Expr>, span }`. Index access
  already exists as `Expr::Index { target, index, span }`.
- **Parser**: parse `[` in `parse_primary` -> `parse_array_literal`
  (`"[" [ expr ("," expr)* [","] ] "]"`). `array[index]` already parses as
  a postfix `Index`.
- **Grammar**: add `array_literal = "[" , [ expr_list ] , "]" ;` to
  `docs/grammar.ebnf` and `array_literal` to `primary_expr`.
- **dump**: add `Expr::Array` arm (e.g. `[a..b] Array` with children).

## Emitter (`capy-emitter`)

- `Expr::Array { elems }` -> emit each element in order, then
  `MakeArray(elems.len())`.
- `Expr::Index { target, index }` -> emit `target`, emit `index`,
  `ArrayGet` (replaces today's `E0001 UnsupportedExpr "index"` rejection).
- **Indexed assignment** `a[i] = v`: extend `emit_assign` so an
  `Expr::Index` target lowers to: emit `target`, emit `index`, emit
  `value`, `ArraySet`, then the statement discards the pushed array via the
  normal expr-statement `Pop`. (Today `emit_assign` rejects non-`Ident`
  targets with `E0021`; arrays become the first supported non-ident
  target.)
- **`len`**: expose array length either as a special-cased builtin call
  `len(a)` lowering to `ArrayLen`, or defer to `capy-stdlib` (S10). Pick
  one in implementation; the opcode exists regardless.

## Exhaustive-match checklist (so the build stays green)

Adding `Value::Array` touches every exhaustive `match` on `Value`. The
implementation MUST update each (else `cargo build` fails):

- `capy-vm/src/value.rs`: `Value::type_name` (add `"array"`).
- `capy-vm/src/execute.rs`: `cmp_equality` (element-wise array eq),
  `cmp_order` (arrays -> `TypeMismatch`), `binop_numeric`, `op_add`,
  `binop_bitwise`, the inline `Neg` / `Not` / `BitNot` arms (arrays ->
  `TypeMismatch`), plus the new `MakeArray` / `ArrayGet` / `ArraySet` /
  `ArrayLen` arms in the main instruction `match`.
- `capyc/src/main.rs`: `format_value` (render arrays, e.g. `[1, 2, 3]`).
- `capy-bytecode`: `Opcode` enum / `from_byte` / `mnemonic`;
  `Instruction` enum / `opcode` / `decode`; `required_inputs` /
  `produced_outputs`; the round-trip test arrays.
- `capy-ast`: `Expr` `span()` and `dump` (the `Expr::Array` arm);
  `is_block_like` is unaffected (arrays are not block-like).

Adding `Rc`/`RefCell` means `Value` is no longer `Copy` (it already is not,
because of `Str(String)`), so no new `Copy` assumptions break.

## Acceptance criteria

- `make rust-validate` green.
- Round-trip + `disassemble_text` for the four opcodes; verifier accepts a
  balanced array program and rejects an unbalanced one.
- VM end-to-end: build `[1, 2, 3]`, `a[1]` -> `2`, `a[0] = 9` then `a[0]`
  -> `9`; out-of-bounds and wrong-type access trap with `V0017` /
  `V0005`; a `for` loop that fills then sums an array.
- A small program that mutates an array inside a `for` loop (proving S2.4 +
  S2.5 + S6.2 compose) - e.g. fill `a[i] = i` for `i in 0..n` then sum.
- Docs updated: `docs/bytecode-v0.md` (opcodes + `V0017`),
  `docs/compatibility.md` (Value model + opcode count + error code),
  `docs/grammar.ebnf` (array literal), `docs/roadmap.md` (mark S6.2 done),
  `CHANGELOG.md` (slice entry).

## Follow-on (S6.3, out of scope here)

Tuples / structs / enum payloads reuse the aggregate machinery
(`MakeTuple`, `GetField`, a tagged variant value) and enable lowering
`struct` / `enum` items and the `match` tuple-struct / struct / path
patterns that parse today but the emitter still rejects.
