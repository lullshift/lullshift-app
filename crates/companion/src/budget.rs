//! Monthly token budget (ADR-020): checked before sending, from recorded usage.

use crate::error::CompanionError;

/// `Ok` while the tokens recorded this UTC month are below the budget. No budget means no
/// limit. A call that starts under the limit may end above it.
pub fn check(budget: Option<i64>, used_input: i64, used_output: i64) -> Result<(), CompanionError> {
    match budget {
        Some(limit) if used_input.saturating_add(used_output) >= limit => {
            Err(CompanionError::budget_reached())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorKind;

    #[test]
    fn blocks_at_or_over_the_limit() {
        assert!(check(None, 1_000_000, 1_000_000).is_ok());
        assert!(check(Some(100), 40, 59).is_ok());
        assert_eq!(
            check(Some(100), 40, 60).unwrap_err().kind,
            ErrorKind::BudgetReached
        );
        assert_eq!(
            check(Some(100), 500, 0).unwrap_err().kind,
            ErrorKind::BudgetReached
        );
        assert!(check(Some(100), i64::MAX, i64::MAX).is_err());
    }
}
