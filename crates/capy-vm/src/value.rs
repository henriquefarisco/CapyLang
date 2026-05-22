//! Runtime value type for the v0 VM.
//!
//! [`Value`] is a tagged enum with a small fixed set of variants
//! mirroring the constant categories ([`Constant`](capy_bytecode::Constant))
//! plus runtime booleans and the unit ("none") sentinel. Strings are
//! held by-value (clone-on-store) for now; an `Rc<str>` optimisation
//! is a future refinement once host ABI hand-off lands.

#![forbid(unsafe_code)]

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
        }
    }
}
