//! Gradual Typing Core
//!
//! Defines the `Ty` type model and small helpers:
//! - Union merging with flattening and deduplication
//! - Subtyping relation for common composite types (arrays, maps, records)
//! - Function type variance rules (params contravariant, return covariant)
//! - Narrowing helpers for null and simple conditions
// Minimal gradual-typing model: Ty, union merge, simple subtyping and narrowing helpers.
// This file is intentionally small and conservative. It will be extended later
// to integrate with the existing AST and the HIR→MIR guard insertion pipeline.

use std::fmt;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Char,
    Str,
    Null,
    Void,
    Never,
    Any,
    Unknown,
    // Fixed-width integer types (unsigned)
    U8,
    U16,
    U32,
    U64,
    U128,
    // Fixed-width integer types (signed)
    I8,
    I16,
    I32,
    I64,
    I128,
    // Fixed-width float types
    F32,
    F64Ty, // Named differently to avoid confusion with the existing Float
    /// Raw pointer to an inner type (e.g., *u8)
    Ptr(Box<Ty>),
    /// Owning pointer - must be freed, move-only
    PtrOwning(Box<Ty>),
    /// Shared read-only reference - copyable
    PtrShared(Box<Ty>),
    /// Unique mutable reference - non-copyable
    PtrMut(Box<Ty>),
    Array(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Record {
        required: Vec<(String, Ty)>,
        optional: Vec<(String, Ty)>,
    },
    Tuple(Vec<Ty>),
    Union(Vec<Ty>),
    Nullable(Box<Ty>), // sugar for Union(T, Null) / T?
    // Nominal and Algebraic Data Types
    Class(String),
    Struct(String),
    Interface(String),
    Enum(String),
    OptionTy(Box<Ty>),
    ResultTy(Box<Ty>, Box<Ty>),
    GenericParam(String),
    GenericInstance {
        name: String,
        args: Vec<Ty>,
    },
    Func {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Ty::*;
        match self {
            Int => write!(f, "i32"),
            Float => write!(f, "f64"),
            Bool => write!(f, "Bool"),
            Char => write!(f, "Char"),
            Str => write!(f, "String"),
            Null => write!(f, "Null"),
            Void => write!(f, "Void"),
            Never => write!(f, "Never"),
            Any => write!(f, "Any"),
            Unknown => write!(f, "Unknown"),
            // Fixed-width integer types (unsigned)
            U8 => write!(f, "u8"),
            U16 => write!(f, "u16"),
            U32 => write!(f, "u32"),
            U64 => write!(f, "u64"),
            U128 => write!(f, "u128"),
            // Fixed-width integer types (signed)
            I8 => write!(f, "i8"),
            I16 => write!(f, "i16"),
            I32 => write!(f, "i32"),
            I64 => write!(f, "i64"),
            I128 => write!(f, "i128"),
            // Fixed-width float types
            F32 => write!(f, "f32"),
            F64Ty => write!(f, "f64"),
            Ptr(t) => write!(f, "*{}", t),
            PtrOwning(t) => write!(f, "ptr<{}>", t),
            PtrShared(t) => write!(f, "ref<{}>", t),
            PtrMut(t) => write!(f, "mutref<{}>", t),
            Array(t) => write!(f, "[{}]", t),
            Map(k, v) => write!(f, "Map<{},{}>", k, v),
            Record { required, optional } => {
                write!(f, "{{")?;
                let mut first = true;
                for (k, t) in required {
                    if !first {
                        write!(f, ", ")?;
                    } else {
                        first = false;
                    }
                    write!(f, "{}: {}", k, t)?;
                }
                for (k, t) in optional {
                    if !first {
                        write!(f, ", ")?;
                    } else {
                        first = false;
                    }
                    write!(f, "{}?: {}", k, t)?;
                }
                write!(f, "}}")
            }
            Tuple(ts) => {
                write!(f, "(")?;
                for (i, t) in ts.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            Union(ts) => {
                let mut first = true;
                for t in ts {
                    if !first {
                        write!(f, " | ")?;
                    } else {
                        first = false;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            Nullable(t) => write!(f, "{}?", t),
            Class(n) => write!(f, "class {}", n),
            Struct(n) => write!(f, "struct {}", n),
            Interface(n) => write!(f, "interface {}", n),
            Enum(n) => write!(f, "enum {}", n),
            OptionTy(t) => write!(f, "Option<{}>", t),
            ResultTy(t, e) => write!(f, "Result<{}, {}>", t, e),
            GenericParam(n) => write!(f, "{}", n),
            GenericInstance { name, args } => {
                write!(f, "{}<", name)?;
                for (i, a) in args.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", a)?;
                }
                write!(f, ">")
            }
            Func { params, ret } => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
        }
    }
}

/// Merge two types into a union, simplifying nested unions and removing duplicates.
pub fn union_merge(a: Ty, b: Ty) -> Ty {
    use Ty::*;
    // fast paths
    if a == b {
        return a;
    }
    if matches!(a, Any) || matches!(b, Any) {
        return Any;
    }

    let mut items: Vec<Ty> = Vec::new();
    fn push_item(items: &mut Vec<Ty>, t: Ty) {
        match t {
            Ty::Union(mut inner) => {
                for i in inner.drain(..) {
                    push_item(items, i);
                }
            }
            other => {
                if !items.iter().any(|x| x == &other) {
                    items.push(other);
                }
            }
        }
    }

    push_item(&mut items, a);
    push_item(&mut items, b);

    if items.len() == 1 {
        return items.into_iter().next().unwrap();
    }
    Ty::Union(items)
}

/// Very small subtyping relation used by early passes.
/// Conservative: Unknown and Any behave as top and wildcard respectively.
pub fn is_subtype(sub: &Ty, sup: &Ty) -> bool {
    use Ty::*;
    if sub == sup {
        return true;
    }
    match (sub, sup) {
        (Int, I32) | (I32, Int) => true,
        (Float, F64Ty) | (F64Ty, Float) => true,
        // Pointer subtyping: only identical inner type allowed
        (Ptr(a), Ptr(b)) => is_subtype(a, b),
        // Owning pointer: only identical types allowed (move semantics)
        (PtrOwning(a), PtrOwning(b)) => is_subtype(a, b),
        // Shared ref: covariant in inner type
        (PtrShared(a), PtrShared(b)) => is_subtype(a, b),
        // Mutable ref: invariant in inner type (for safety)
        (PtrMut(a), PtrMut(b)) => a == b,
        // Tuple subtyping: same arity, pairwise subtyping
        (Tuple(sa), Tuple(sb)) => {
            if sa.len() != sb.len() {
                return false;
            }
            for (a, b) in sa.iter().zip(sb.iter()) {
                if !is_subtype(a, b) {
                    return false;
                }
            }
            true
        }
        (_, GenericParam(_)) => true,
        (GenericParam(_), _) => true,
        (_, Any) => true,
        (Unknown, _) => true,
        (_, Unknown) => true, // Unknown is top here for safety in some passes
        (Class(a), Class(b)) => a == b,
        (Struct(a), Struct(b)) => a == b,
        (Interface(a), Interface(b)) => a == b,
        (Enum(a), Enum(b)) => a == b,
        (OptionTy(a), OptionTy(b)) => is_subtype(a, b),
        (ResultTy(t1, e1), ResultTy(t2, e2)) => is_subtype(t1, t2) && is_subtype(e1, e2),
        (Null, OptionTy(_)) => true,
        (OptionTy(s), sup) => is_subtype(s, sup) || *sup == Ty::Null,
        (sub, OptionTy(target)) => is_subtype(sub, target) || *sub == Ty::Null,
        (Null, Nullable(_)) => true,
        (Nullable(s), Union(choices)) => choices.iter().any(|c| is_subtype(s, c) || *c == Ty::Null),
        (Nullable(s), sup) => is_subtype(s, sup) && is_subtype(&Ty::Null, sup),
        (sub, Nullable(target)) => is_subtype(sub, target) || *sub == Ty::Null,
        (Union(sources), sup) => sources.iter().all(|c| is_subtype(c, sup)),
        (Array(s), Array(t)) => {
            matches!(**s, Ty::Unknown | Ty::Never | Ty::Any) || is_subtype(s, t)
        }
        (Record { required, optional }, GenericInstance { name, .. })
            if required.is_empty()
                && optional.is_empty()
                && (name == "Set" || name == "Dict" || name == "Map") =>
        {
            true
        }
        (Array(inner), GenericInstance { name, .. })
            if matches!(**inner, Ty::Unknown | Ty::Never)
                && (name == "Set" || name == "Array" || name == "List") =>
        {
            true
        }
        (Record { required, optional }, Map(_, _))
            if required.is_empty() && optional.is_empty() =>
        {
            true
        }
        (GenericInstance { name: na, args: aa }, GenericInstance { name: nb, args: ab }) => {
            if na != nb {
                return false;
            }
            if aa.len() != ab.len() {
                return false;
            }
            for (x, y) in aa.iter().zip(ab.iter()) {
                if !is_subtype(x, y) {
                    return false;
                }
            }
            true
        }
        (
            Record {
                required: sreq,
                optional: sopt,
            },
            Record {
                required: treq,
                optional: _topt,
            },
        ) => {
            // structural subtyping: every required field in target must exist in source
            for (k, t_ty) in treq {
                // look for k in source required or optional
                let mut found: Option<&Ty> = None;
                for (sk, st) in sreq {
                    if sk == k {
                        found = Some(st);
                        break;
                    }
                }
                if found.is_none() {
                    for (sk, st) in sopt {
                        if sk == k {
                            found = Some(st);
                            break;
                        }
                    }
                }
                if let Some(ft) = found {
                    if !is_subtype(ft, t_ty) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            // optional fields in target may be missing in source; accept
            true
        }
        (
            Map(_ks, vs),
            Record {
                required: treq,
                optional: _topt,
            },
        ) => {
            // If source is a Map<Str, V>, ensure V is subtype of each required field type
            for (_k, t_ty) in treq {
                if !is_subtype(vs, t_ty) {
                    return false;
                }
            }
            true
        }
        (
            Record {
                required: sreq,
                optional: sopt,
            },
            Map(kt, vt),
        ) => {
            // A Record is a subtype of Map<kt, vt> if kt is String/Str,
            // and every field type in sreq and sopt is a subtype of vt.
            if **kt == Ty::Str {
                for (_, st) in sreq {
                    if !is_subtype(st, vt) {
                        return false;
                    }
                }
                for (_, st) in sopt {
                    if !is_subtype(st, vt) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        (Map(ks, vs), Map(kt, vt)) => is_subtype(ks, kt) && is_subtype(vs, vt),
        (
            Func {
                params: p1,
                ret: r1,
            },
            Func {
                params: p2,
                ret: r2,
            },
        ) => {
            // params contravariant (simple length & pairwise), returns covariant
            if p1.len() != p2.len() {
                return false;
            }
            for (a, b) in p1.iter().zip(p2.iter()) {
                if !is_subtype(b, a) {
                    return false;
                }
            }
            is_subtype(r1, r2)
        }
        _ => false,
    }
}

/// Narrow a variable type based on a null-check condition. If `is_eq_null` is true,
/// we are in the branch where `var == null` and should return `Null` or a union containing Null.
/// If false, we are in the non-null branch and should remove Null from the type.
pub fn narrow_on_null(var_ty: &Ty, is_eq_null: bool) -> Ty {
    use Ty::*;
    match (var_ty, is_eq_null) {
        (Nullable(_inner), true) => Ty::Null,
        (Nullable(inner), false) => *inner.clone(),
        (GenericParam(_), true) => Ty::Null,
        (GenericParam(_), false) => Ty::Any,
        (GenericInstance { .. }, true) => Ty::Null,
        (GenericInstance { .. }, false) => var_ty.clone(),
        (Union(choices), true) => {
            // keep only Null if present, else Any
            if choices.contains(&Null) { Null } else { Any }
        }
        (Union(choices), false) => {
            let filtered: Vec<Ty> = choices.iter().filter(|&c| *c != Null).cloned().collect();
            if filtered.is_empty() {
                Any
            } else if filtered.len() == 1 {
                filtered.into_iter().next().unwrap()
            } else {
                Union(filtered)
            }
        }
        (t, true) => {
            if *t == Null {
                Null
            } else {
                Any
            }
        }
        (t, false) => t.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union_merge_flatten_and_dedup() {
        let a = Ty::Int;
        let b = Ty::Null;
        let u = union_merge(a.clone(), b.clone());
        assert!(matches!(u, Ty::Union(_)));
        // merging same again yields same
        let u2 = union_merge(u.clone(), a.clone());
        assert!(matches!(u2, Ty::Union(_)));
    }

    #[test]
    fn test_subtyping_basic() {
        assert!(is_subtype(&Ty::Int, &Ty::Any));
        assert!(!is_subtype(&Ty::Any, &Ty::Int));
        assert!(is_subtype(
            &Ty::Nullable(Box::new(Ty::Int)),
            &Ty::Union(vec![Ty::Int, Ty::Null])
        ));
    }

    #[test]
    fn test_narrow_on_null() {
        let t = Ty::Nullable(Box::new(Ty::Str));
        let non_null = narrow_on_null(&t, false);
        assert_eq!(non_null, Ty::Str);
        let is_null = narrow_on_null(&t, true);
        assert_eq!(is_null, Ty::Null);
    }

    /// Test fixed-width numeric types display
    #[test]
    fn test_fixed_width_types_display() {
        assert_eq!(format!("{}", Ty::U8), "u8");
        assert_eq!(format!("{}", Ty::U16), "u16");
        assert_eq!(format!("{}", Ty::U32), "u32");
        assert_eq!(format!("{}", Ty::U64), "u64");
        assert_eq!(format!("{}", Ty::U128), "u128");
        assert_eq!(format!("{}", Ty::I8), "i8");
        assert_eq!(format!("{}", Ty::I16), "i16");
        assert_eq!(format!("{}", Ty::I32), "i32");
        assert_eq!(format!("{}", Ty::I64), "i64");
        assert_eq!(format!("{}", Ty::I128), "i128");
        assert_eq!(format!("{}", Ty::F32), "f32");
        assert_eq!(format!("{}", Ty::F64Ty), "f64");
    }

    /// Test subtyping for fixed-width types
    #[test]
    fn test_fixed_width_subtyping() {
        // Fixed-width types should subtype to Any
        assert!(is_subtype(&Ty::U8, &Ty::Any));
        assert!(is_subtype(&Ty::I32, &Ty::Any));
        assert!(is_subtype(&Ty::F32, &Ty::Any));

        // Same types should be subtypes of each other
        assert!(is_subtype(&Ty::U8, &Ty::U8));
        assert!(is_subtype(&Ty::I32, &Ty::I32));
        assert!(is_subtype(&Ty::F64Ty, &Ty::F64Ty));
    }
}
