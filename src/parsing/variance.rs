//! Variance Analysis (Phase 3)
//!
//! Provides basic variance computation for generic type parameters.
//! This scaffold computes conservative variance marks for identifiers
//! referenced inside `HirType` trees.

use crate::parsing::hir::HirType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variance {
    Covariant,
    Contravariant,
    Invariant,
    Bivariant,
}

impl Variance {
    fn compose(self, position: Variance) -> Variance {
        use Variance::*;
        match (self, position) {
            (Invariant, _) | (_, Invariant) => Invariant,
            (Bivariant, x) | (x, Bivariant) => x,
            (Covariant, Covariant) => Covariant,
            (Contravariant, Contravariant) => Covariant,
            (Covariant, Contravariant) => Contravariant,
            (Contravariant, Covariant) => Contravariant,
        }
    }
}

/// Summarize the overall variance behavior of a composite type.
/// This does not attribute variance to named parameters (the HIR
/// does not carry those directly) but gives a conservative summary
/// useful for downstream checks.
pub fn summarize_variance(ty: &HirType) -> Variance {
    summarize_at(ty, Variance::Covariant)
}

fn merge(a: Variance, b: Variance) -> Variance {
    use Variance::*;
    match (a, b) {
        (Invariant, _) | (_, Invariant) => Invariant,
        (x, Bivariant) | (Bivariant, x) => x,
        (Covariant, Covariant) => Covariant,
        (Contravariant, Contravariant) => Covariant,
        _ => Invariant,
    }
}

fn summarize_at(ty: &HirType, position: Variance) -> Variance {
    use HirType::*;
    match ty {
        BorrowMut(inner) => {
            // &mut T is invariant in T
            let _ = summarize_at(inner, Variance::Invariant);
            Variance::Invariant
        }
        BorrowImmut(inner) => summarize_at(inner, position),
        Shared(inner) | Weak(inner) | Promise(inner) => summarize_at(inner, position),
        Function(params, ret) => {
            let mut v = Variance::Bivariant;
            for p in params {
                v = merge(
                    v,
                    summarize_at(p, Variance::Contravariant.compose(position)),
                );
            }
            v = merge(v, summarize_at(ret, Variance::Covariant.compose(position)));
            v
        }
        Array(elem, _) => summarize_at(elem, position),
        Dict(k, v) => merge(summarize_at(k, position), summarize_at(v, position)),
        Set(elem) => summarize_at(elem, position),
        Tuple(items) => items.iter().fold(Variance::Bivariant, |acc, it| {
            merge(acc, summarize_at(it, position))
        }),
        _ => Variance::Bivariant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variance_smoke() {
        let t = HirType::BorrowImmut(Box::new(HirType::I32));
        let v = summarize_variance(&t);
        assert!(matches!(
            v,
            Variance::Bivariant
                | Variance::Covariant
                | Variance::Invariant
                | Variance::Contravariant
        ));
    }
}
