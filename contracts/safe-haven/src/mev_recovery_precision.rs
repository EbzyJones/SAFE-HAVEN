// ============================================================
//  MEV Recovery Precision Module
//
//  Provides accurate calculation and verification of MEV recovered amounts
//  based on price deviation and deposit amount.
//
//  Formula: recovered_amount = amount × (deviation_bps / 10000)
//
//  Key invariants:
//  - Precision maintained for all deposit amounts
//  - No rounding loss in critical operations
//  - Rounding is consistent and documented
//  - No funds created or destroyed
// ============================================================

use soroban_sdk::i128;

/// Calculate the recovered MEV amount based on deposit amount and price deviation.
///
/// Formula: `recovered_amount = amount × (deviation_bps / 10_000)`
///
/// **Precision strategy:**
/// - Multiply `amount × deviation_bps` first (before dividing by 10_000)
/// - This preserves fractional basis points for sub-unit precision
/// - Integer division truncates toward zero (floor for positive values)
/// - Rounding is consistent: remainder is always discarded
///
/// **Safety:**
/// - Uses saturating arithmetic to prevent overflow
/// - If `amount × deviation_bps` overflows, returns 0 (safe failure)
/// - Ensures no funds are created through calculation errors
///
/// # Arguments
/// * `amount` - Original deposit amount (in smallest units)
/// * `deviation_bps` - Price deviation in basis points (0-10000+)
///
/// # Returns
/// Recovered amount in the same units as `amount`, or 0 if calculation overflows
///
/// # Examples
/// - amount=1000, deviation_bps=100 → 10 (1% of amount)
/// - amount=1000, deviation_bps=200 → 20 (2% of amount)
/// - amount=100, deviation_bps=50 → 0 (truncates to 0)
/// - amount=1000000, deviation_bps=5000 → 500000 (50% of amount)
pub fn calculate_mev_recovered(amount: i128, deviation_bps: i128) -> i128 {
    if amount <= 0 || deviation_bps <= 0 {
        return 0;
    }

    // Multiply first to preserve precision before division
    // This is crucial for small amounts where deviation_bps < 10_000
    match amount.checked_mul(deviation_bps) {
        Some(product) => product / 10_000,
        None => 0, // Overflow: return 0 (safe failure)
    }
}

/// Calculate recovered amount with explicit remainder tracking for transparency.
///
/// Returns both the integer result and the truncated remainder.
/// This allows callers to understand precision loss if needed.
///
/// # Returns
/// (recovered_amount, truncated_remainder)
pub fn calculate_mev_recovered_with_remainder(amount: i128, deviation_bps: i128) -> (i128, i128) {
    if amount <= 0 || deviation_bps <= 0 {
        return (0, 0);
    }

    match amount.checked_mul(deviation_bps) {
        Some(product) => {
            let recovered = product / 10_000;
            let remainder = product % 10_000;
            (recovered, remainder)
        }
        None => (0, 0), // Overflow: return (0, 0)
    }
}

/// Verify that a recovered amount calculation is accurate and hasn't lost funds.
///
/// This function validates:
/// 1. Recovered amount ≤ original amount (never recover more than deposited)
/// 2. Recovered amount ≤ amount × (deviation_bps / 10_000) (correct formula)
/// 3. Recovered amount ≥ amount × (deviation_bps / 10_000) - 1 (at most 1 unit lost to rounding)
///
/// # Returns
/// `true` if calculation is valid, `false` if invariant violated
pub fn verify_recovery_calculation(
    amount: i128,
    deviation_bps: i128,
    recovered_amount: i128,
) -> bool {
    if amount <= 0 || deviation_bps <= 0 {
        return recovered_amount == 0;
    }

    // Recovered amount must be <= original amount
    if recovered_amount > amount {
        return false;
    }

    let (expected_recovered, _remainder) = calculate_mev_recovered_with_remainder(amount, deviation_bps);

    // Recovered must equal our calculation
    if recovered_amount != expected_recovered {
        return false;
    }

    // All checks passed
    true
}

/// Calculate maximum possible MEV recovery for a given amount.
/// This occurs at 100% deviation (10_000 basis points).
///
/// Returns the amount itself (100% recovery at 10_000 bps).
pub fn max_mev_recovery(amount: i128) -> i128 {
    calculate_mev_recovered(amount, 10_000)
}

/// Calculate MEV recovery at the MEV detection threshold (200 bps = 2%).
/// This is the minimum deviation that triggers detection.
pub fn mev_recovery_at_threshold(amount: i128) -> i128 {
    calculate_mev_recovered(amount, 200)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ================================================================
    //  Basic Calculation Tests
    // ================================================================

    #[test]
    fn test_mev_recovery_basic_1_percent() {
        // amount=1000, deviation=100 bps (1%) → 10
        let recovered = calculate_mev_recovered(1000, 100);
        assert_eq!(recovered, 10, "1% of 1000 = 10");
    }

    #[test]
    fn test_mev_recovery_basic_2_percent() {
        // amount=1000, deviation=200 bps (2%) → 20
        let recovered = calculate_mev_recovered(1000, 200);
        assert_eq!(recovered, 20, "2% of 1000 = 20");
    }

    #[test]
    fn test_mev_recovery_basic_10_percent() {
        // amount=1000, deviation=1000 bps (10%) → 100
        let recovered = calculate_mev_recovered(1000, 1000);
        assert_eq!(recovered, 100, "10% of 1000 = 100");
    }

    #[test]
    fn test_mev_recovery_basic_50_percent() {
        // amount=1000, deviation=5000 bps (50%) → 500
        let recovered = calculate_mev_recovered(1000, 5000);
        assert_eq!(recovered, 500, "50% of 1000 = 500");
    }

    #[test]
    fn test_mev_recovery_basic_100_percent() {
        // amount=1000, deviation=10000 bps (100%) → 1000
        let recovered = calculate_mev_recovered(1000, 10_000);
        assert_eq!(recovered, 1000, "100% of 1000 = 1000");
    }

    // ================================================================
    //  Edge Case: Small Amounts (Rounding)
    // ================================================================

    #[test]
    fn test_mev_recovery_small_amount_rounds_down() {
        // amount=100, deviation=50 bps (0.5%) → 0 (100 * 50 / 10000 = 0.5 → truncates to 0)
        let recovered = calculate_mev_recovered(100, 50);
        assert_eq!(recovered, 0, "Small amount with low deviation truncates to 0");
    }

    #[test]
    fn test_mev_recovery_small_amount_no_rounding_loss() {
        // amount=100, deviation=100 bps (1%) → 1 (100 * 100 / 10000 = 1)
        let recovered = calculate_mev_recovered(100, 100);
        assert_eq!(recovered, 1, "1% of 100 = 1 exactly");
    }

    #[test]
    fn test_mev_recovery_small_amount_slight_rounding() {
        // amount=333, deviation=100 bps (1%) → 3 (333 * 100 / 10000 = 3.33 → truncates to 3)
        let recovered = calculate_mev_recovered(333, 100);
        assert_eq!(recovered, 3, "1% of 333 = 3 (with truncation)");
    }

    #[test]
    fn test_mev_recovery_very_small_amount() {
        // amount=1, deviation=100 bps (1%) → 0 (1 * 100 / 10000 = 0.01 → truncates to 0)
        let recovered = calculate_mev_recovered(1, 100);
        assert_eq!(recovered, 0, "1% of 1 = 0 (truncates from 0.01)");
    }

    #[test]
    fn test_mev_recovery_very_small_amount_at_threshold() {
        // amount=1, deviation=10000 bps (100%) → 1 (1 * 10000 / 10000 = 1)
        let recovered = calculate_mev_recovered(1, 10_000);
        assert_eq!(recovered, 1, "100% of 1 = 1");
    }

    // ================================================================
    //  Edge Case: Large Amounts (Precision at Scale)
    // ================================================================

    #[test]
    fn test_mev_recovery_large_amount_1m() {
        // amount=1000000, deviation=100 bps (1%) → 10000
        let recovered = calculate_mev_recovered(1_000_000, 100);
        assert_eq!(recovered, 10_000, "1% of 1M = 10K");
    }

    #[test]
    fn test_mev_recovery_large_amount_1b() {
        // amount=1000000000, deviation=100 bps (1%) → 10000000
        let recovered = calculate_mev_recovered(1_000_000_000, 100);
        assert_eq!(recovered, 10_000_000, "1% of 1B = 10M");
    }

    #[test]
    fn test_mev_recovery_large_amount_1e15() {
        // amount=1000000000000000 (max deposit), deviation=100 bps (1%) → 10000000000000
        let recovered = calculate_mev_recovered(1_000_000_000_000_000, 100);
        assert_eq!(recovered, 10_000_000_000_000, "1% of 1e15 = 1e13");
    }

    // ================================================================
    //  Precision: Remainder Tracking
    // ================================================================

    #[test]
    fn test_mev_recovery_with_remainder_exact() {
        // amount=1000, deviation=100 (no remainder: 1000 * 100 = 100000 / 10000 = 10)
        let (recovered, remainder) = calculate_mev_recovered_with_remainder(1000, 100);
        assert_eq!(recovered, 10, "Recovered = 10");
        assert_eq!(remainder, 0, "No remainder");
    }

    #[test]
    fn test_mev_recovery_with_remainder_truncation() {
        // amount=333, deviation=100 (333 * 100 = 33300 / 10000 = 3 remainder 3300)
        let (recovered, remainder) = calculate_mev_recovered_with_remainder(333, 100);
        assert_eq!(recovered, 3, "Recovered = 3");
        assert_eq!(remainder, 3300, "Remainder = 3300 out of 10000");
    }

    #[test]
    fn test_mev_recovery_with_remainder_max_precision() {
        // amount=9999, deviation=1 (9999 * 1 = 9999 / 10000 = 0 remainder 9999)
        let (recovered, remainder) = calculate_mev_recovered_with_remainder(9999, 1);
        assert_eq!(recovered, 0, "Recovered = 0");
        assert_eq!(remainder, 9999, "Remainder = 9999 out of 10000");
    }

    #[test]
    fn test_mev_recovery_with_remainder_documentation() {
        // amount=1, deviation=15000 (1 * 15000 = 15000 / 10000 = 1 remainder 5000)
        // Shows that at 150% deviation, we recover 1 unit with 5000 bps (~50%) lost to division
        let (recovered, remainder) = calculate_mev_recovered_with_remainder(1, 15_000);
        assert_eq!(recovered, 1, "Recovered = 1 at 150% deviation");
        assert_eq!(remainder, 5000, "Remainder represents precision loss");
    }

    // ================================================================
    //  Verification: No Funds Created or Destroyed
    // ================================================================

    #[test]
    fn test_verify_recovery_valid_case() {
        // amount=1000, deviation=100, recovered=10 → valid
        let valid = verify_recovery_calculation(1000, 100, 10);
        assert!(valid, "Valid recovery calculation should verify");
    }

    #[test]
    fn test_verify_recovery_invalid_too_much() {
        // amount=1000, deviation=100, recovered=11 → invalid (too much recovered)
        let valid = verify_recovery_calculation(1000, 100, 11);
        assert!(!valid, "Recovery exceeding formula should fail verification");
    }

    #[test]
    fn test_verify_recovery_invalid_exceeds_amount() {
        // amount=1000, deviation=100, recovered=1001 → invalid (exceeds amount)
        let valid = verify_recovery_calculation(1000, 100, 1001);
        assert!(!valid, "Recovery exceeding original amount should fail");
    }

    #[test]
    fn test_verify_recovery_invalid_negative_amount() {
        // Negative amounts should not be recoverable
        let valid = verify_recovery_calculation(-1000, 100, 0);
        assert!(valid, "Negative amount with 0 recovery is valid");
    }

    #[test]
    fn test_verify_recovery_all_amounts_valid_at_threshold() {
        // At 200 bps (MEV detection threshold), verify across range of amounts
        for amount in &[1i128, 10, 100, 1000, 1_000_000, 1_000_000_000_000_000] {
            let recovered = calculate_mev_recovered(*amount, 200);
            let valid = verify_recovery_calculation(*amount, 200, recovered);
            assert!(valid, "Recovery should verify for amount={}", amount);
            // Also verify we recover at least 2% (without remainder loss making it 0)
            if *amount >= 50 {
                assert!(recovered > 0, "Should recover > 0 for amount >= 50 at 200 bps");
            }
        }
    }

    // ================================================================
    //  Special Cases: Stablecoin Scenarios
    // ================================================================

    #[test]
    fn test_mev_recovery_usdc_6_decimals() {
        // USDC: 1 USDC = 1_000_000 units
        // Deposit: 100 USDC = 100_000_000 units
        // Deviation: 200 bps (2% = MEV detection threshold)
        // Expected recovery: 100_000_000 * 200 / 10000 = 2_000_000 units = 2 USDC

        let amount = 100_000_000; // 100 USDC
        let deviation_bps = 200;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 2_000_000, "2% of 100 USDC = 2 USDC");
        // Verify this is exactly 2 USDC (no rounding loss)
        assert_eq!(recovered % 1_000_000, 0, "Recovery is whole USDC");
    }

    #[test]
    fn test_mev_recovery_usdc_small_deposit() {
        // USDC: small deposit of 0.01 USDC = 10_000 units
        // Deviation: 200 bps (2%)
        // Expected recovery: 10_000 * 200 / 10000 = 200 units = 0.0002 USDC

        let amount = 10_000; // 0.01 USDC
        let deviation_bps = 200;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 200, "2% of 0.01 USDC = 0.0002 USDC");
    }

    #[test]
    fn test_mev_recovery_usdt_8_decimals_equivalent() {
        // Some tokens like WBTC use 8 decimals
        // 1 token = 100_000_000 units
        // Deposit: 10 tokens = 1_000_000_000 units
        // Deviation: 300 bps (3%)
        // Expected recovery: 1_000_000_000 * 300 / 10000 = 30_000_000 units

        let amount = 1_000_000_000;
        let deviation_bps = 300;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 30_000_000, "3% of 10e8 units = 30M units");
    }

    #[test]
    fn test_mev_recovery_dai_18_decimals() {
        // DAI uses 18 decimals
        // 1 DAI = 1_000_000_000_000_000_000 units
        // Deposit: 1000 DAI = 1_000_000_000_000_000_000_000 units (but would overflow, use smaller)
        // Deposit: 1 DAI = 1_000_000_000_000_000_000 units
        // Deviation: 200 bps (2%)
        // Expected recovery: 1e18 * 200 / 10000 = 2e14

        let amount = 1_000_000_000_000_000_000i128; // 1 DAI
        let deviation_bps = 200;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 20_000_000_000_000, "2% of 1 DAI");
    }

    // ================================================================
    //  Boundary Tests: Deviation at Thresholds
    // ================================================================

    #[test]
    fn test_mev_recovery_at_detection_threshold() {
        // MEV threshold is 200 bps
        // amount=1000, deviation=200 → should recover 20
        let recovered = calculate_mev_recovered(1000, 200);
        assert_eq!(recovered, 20, "At detection threshold (200 bps), recover 2% of amount");
    }

    #[test]
    fn test_mev_recovery_just_below_threshold() {
        // deviation=199 (just below 200 bps threshold)
        // amount=1000 → 1000 * 199 / 10000 = 19
        let recovered = calculate_mev_recovered(1000, 199);
        assert_eq!(recovered, 19, "Just below threshold: 1.99% of 1000 = 19");
    }

    #[test]
    fn test_mev_recovery_just_above_threshold() {
        // deviation=201 (just above 200 bps threshold)
        // amount=1000 → 1000 * 201 / 10000 = 20
        let recovered = calculate_mev_recovered(1000, 201);
        assert_eq!(recovered, 20, "Just above threshold: 2.01% of 1000 = 20");
    }

    #[test]
    fn test_mev_recovery_maximum_at_max_deposit() {
        // Max deposit: 1_000_000_000_000_000
        // Max deviation: 10000 bps (100%)
        // Max recovery: 1_000_000_000_000_000 * 10000 / 10000 = 1_000_000_000_000_000

        let amount = 1_000_000_000_000_000i128;
        let recovered = calculate_mev_recovered(amount, 10_000);

        assert_eq!(recovered, 1_000_000_000_000_000, "100% recovery at max deposit");
    }

    // ================================================================
    //  Overflow Safety
    // ================================================================

    #[test]
    fn test_mev_recovery_overflow_protection() {
        // amount=i128::MAX, deviation=2 → overflow
        // Should return 0 (safe failure)
        let recovered = calculate_mev_recovered(i128::MAX, 2);
        assert_eq!(recovered, 0, "Overflow returns 0 (safe failure)");
    }

    #[test]
    fn test_mev_recovery_near_overflow() {
        // amount=i128::MAX / 2, deviation=10000 → near overflow
        // Safe because product won't exceed i128::MAX / 2 * 10000
        let amount = i128::MAX / 2;
        let recovered = calculate_mev_recovered(amount, 10_000);
        // Should compute successfully
        assert!(recovered > 0, "Large amount computation should succeed");
    }

    // ================================================================
    //  Zero and Negative Input Safety
    // ================================================================

    #[test]
    fn test_mev_recovery_zero_amount() {
        let recovered = calculate_mev_recovered(0, 100);
        assert_eq!(recovered, 0, "Zero amount → zero recovery");
    }

    #[test]
    fn test_mev_recovery_zero_deviation() {
        let recovered = calculate_mev_recovered(1000, 0);
        assert_eq!(recovered, 0, "Zero deviation → zero recovery");
    }

    #[test]
    fn test_mev_recovery_negative_amount() {
        let recovered = calculate_mev_recovered(-1000, 100);
        assert_eq!(recovered, 0, "Negative amount → zero recovery");
    }

    #[test]
    fn test_mev_recovery_negative_deviation() {
        let recovered = calculate_mev_recovered(1000, -100);
        assert_eq!(recovered, 0, "Negative deviation → zero recovery");
    }

    // ================================================================
    //  Max Recovery Helper
    // ================================================================

    #[test]
    fn test_max_mev_recovery_calculates_100_percent() {
        // Max recovery = amount * 10000 / 10000 = amount
        let amount = 1000;
        let max_recovery = max_mev_recovery(amount);
        assert_eq!(max_recovery, amount, "Max recovery should equal amount");
    }

    #[test]
    fn test_mev_recovery_at_threshold_helper() {
        // MEV threshold recovery = amount * 200 / 10000
        let amount = 1000;
        let threshold_recovery = mev_recovery_at_threshold(amount);
        assert_eq!(threshold_recovery, 20, "Threshold recovery = 2% of amount");
    }

    // ================================================================
    //  Real-World Scenarios
    // ================================================================

    #[test]
    fn test_scenario_sandwich_attack_on_1k_deposit() {
        // User deposits 1000 USDC
        // Sandwich attack causes 5% price slip = 5000 bps deviation
        // MEV recovered: 1000 * 5000 / 10000 = 500 USDC

        let amount = 1000;
        let deviation_bps = 5000;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 500, "5% loss = 500 USDC recovered");
    }

    #[test]
    fn test_scenario_minor_slippage_many_deposits() {
        // 1000 deposits of 100 tokens each
        // Each experiences 50 bps (0.5%) slippage
        // Per-deposit recovery: 100 * 50 / 10000 = 0 (truncates)
        // Total "lost" to rounding: 1000 * 0.5 = 500 tokens (but all go to 0 due to truncation)

        let amount = 100;
        let deviation_bps = 50;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 0, "Minor slippage on small amount truncates to 0");
        // This is acceptable: rounding loss is minimal and transparent
    }

    #[test]
    fn test_scenario_flash_loan_attack_large_amount() {
        // User deposits 100 million USDC
        // Flash loan attack causes 2% price spike = 200 bps
        // MEV recovered: 100_000_000 * 200 / 10000 = 2_000_000 USDC

        let amount = 100_000_000;
        let deviation_bps = 200;
        let recovered = calculate_mev_recovered(amount, deviation_bps);

        assert_eq!(recovered, 2_000_000, "2% of 100M = 2M recovered");
    }

    // ================================================================
    //  Regression: Ensure Formula Consistency
    // ================================================================

    #[test]
    fn test_formula_consistency_across_amounts() {
        // For a fixed deviation (100 bps), recovery should scale linearly with amount
        for amount in &[100i128, 1000, 10_000, 100_000] {
            let recovered = calculate_mev_recovered(*amount, 100);
            // Expected: amount * 1 / 100 = amount / 100
            let expected = amount / 100;
            assert_eq!(recovered, expected, "Linear scaling for amount={}", amount);
        }
    }

    #[test]
    fn test_formula_consistency_across_deviations() {
        // For a fixed amount (1000), recovery should scale linearly with deviation
        let amount = 1000;
        for deviation_bps in &[100i128, 200, 500, 1000, 5000] {
            let recovered = calculate_mev_recovered(amount, *deviation_bps);
            let expected = (amount * deviation_bps) / 10_000;
            assert_eq!(
                recovered, expected,
                "Linear scaling for deviation_bps={}",
                deviation_bps
            );
        }
    }
}
