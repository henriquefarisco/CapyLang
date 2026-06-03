//! Runtime value type for the v0 VM.
//!
//! [`Value`] is a tagged enum with a small fixed set of variants
//! mirroring the constant categories ([`Constant`](capy_bytecode::Constant))
//! plus runtime booleans and the unit ("none") sentinel. Strings are
//! held by-value (clone-on-store) for now; an `Rc<str>` optimisation
//! is a future refinement once host ABI hand-off lands.

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

/// A value that lives on the VM's evaluation stack or in a function's
/// locals slot.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Unit / "no value".
    None,
    /// Boolean.
    Bool(bool),
    /// 64-bit signed integer. Arithmetic uses wrapping semantics.
    Int(i64),
    /// 64-bit IEEE-754 float.
    Float(f64),
    /// Owned string. Clone-on-store for v0; will move to a shared
    /// representation in a future refinement.
    Str(String),
    /// Heap array with **reference semantics** (S6.2): a bound array is
    /// mutated in place and aliases share one backing store. The handle
    /// is opaque to bytecode — no host pointer crosses the boundary.
    /// The derived `PartialEq` is element-wise (used by host-side tests);
    /// the VM's in-language comparison opcodes (`Eq` / `Ne` / ordering)
    /// trap on arrays with `TYPE_MISMATCH` in v0.
    Array(Rc<RefCell<Vec<Value>>>),
    /// Tagged aggregate with **reference semantics** (S6.3): a struct
    /// instance or an enum variant. `tag` is an emitter-assigned,
    /// wire-opaque discriminant the VM only stores and compares (all
    /// naming lives in the emitter); `fields` holds the components in
    /// declaration order (struct fields, or the variant payload). Shares
    /// the array reference-semantics / opacity rules: aliases share one
    /// backing store, no host pointer crosses the boundary, the derived
    /// `PartialEq` is structural (host-side tests only) and the
    /// in-language comparison opcodes trap with `TYPE_MISMATCH` in v0.
    Aggregate {
        tag: u32,
        fields: Rc<RefCell<Vec<Value>>>,
    },
}

impl Value {
    /// Human-readable type tag, used inside [`crate::VmError`] payloads.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "str",
            Self::Array(_) => "array",
            Self::Aggregate { .. } => "aggregate",
        }
    }
}
