//! Borrowing Inference (Phase 3)
//!
//! Infers simple borrow modes for function parameters from their types.
//! This is a heuristic, intentionally conservative, and can be improved.

use crate::parsing::hir::HirType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowMode {
    Move,
    SharedBorrow,
    MutBorrow,
}

/// Infer borrow modes from parameter `HirType`s.
///
/// Rules (conservative):
/// - `&T` → SharedBorrow
/// - `&mut T` → MutBorrow
/// - otherwise → Move
pub fn infer_borrow_modes(params: &[HirType]) -> Vec<BorrowMode> {
    params
        .iter()
        .map(|t| match t {
            HirType::BorrowMut(_) => BorrowMode::MutBorrow,
            HirType::BorrowImmut(_) => BorrowMode::SharedBorrow,
            _ => BorrowMode::Move,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_smoke() {
        let params = vec![
            HirType::BorrowImmut(Box::new(HirType::I32)),
            HirType::BorrowMut(Box::new(HirType::I32)),
            HirType::I32,
        ];
        let modes = infer_borrow_modes(&params);
        assert_eq!(
            modes,
            vec![
                BorrowMode::SharedBorrow,
                BorrowMode::MutBorrow,
                BorrowMode::Move
            ]
        );
    }
}
