# MEV Recovery Precision — Quick Reference

## Formula

```
recovered_amount = amount × (deviation_bps / 10_000)
```

**Where:**
- `amount`: Deposit size in token units (e.g., wei for 18-decimal tokens)
- `deviation_bps`: Price deviation in basis points (1 bps = 0.01%)
- `recovered_amount`: MEV recovered in same units

## Examples

### USDC (6 decimals, $1 unit)
```
100 USDC deposit, 2% MEV attack:
100_000_000 units × 200 bps / 10_000 = 2_000_000 units = $2 ✓
```

### Stablecoin Small Amount
```
0.01 USDC deposit, 2% MEV attack:
10_000 units × 200 bps / 10_000 = 200 units = $0.0002 ✓
```

### Large Attack
```
100M USDC deposit, 5% MEV attack:
100_000_000_000_000 × 5000 / 10_000 = 500_000_000_000 units = $500M ✓
```

## Key Properties

| Property | Value |
|----------|-------|
| **Formula** | `amount × deviation_bps / 10_000` |
| **Precision** | Multiply-first division |
| **Rounding** | Truncate (floor) — never over-recover |
| **Max loss** | < 10_000 units (< 1 cent for most tokens) |
| **Overflow** | Returns 0 (safe failure) |
| **MEV detection threshold** | 200 bps (minimum detected) |
| **100% recovery** | 10_000 bps (maximum possible) |

## Code Usage

### Basic Calculation

```rust
use mev_recovery_precision::*;

let amount = 1_000_000_000; // 1000 USDC with 6 decimals
let deviation_bps = 200;    // 2% MEV

let recovered = calculate_mev_recovered(amount, deviation_bps);
// recovered = 20_000_000 (20 USDC)
```

### With Remainder Tracking

```rust
use mev_recovery_precision::*;

let (recovered, remainder) = calculate_mev_recovered_with_remainder(333, 100);
// recovered = 3
// remainder = 3300 (out of 10_000, shows rounding loss)
```

### Verification

```rust
use mev_recovery_precision::*;

let valid = verify_recovery_calculation(1_000_000_000, 200, 20_000_000);
// true if calculation is correct
// false if over-recovery or formula violation
```

### In Contract

```rust
// When MEV is detected:
if deviation_bps > MEV_THRESHOLD_BPS {
    let recovered = calculate_mev_recovered(deposit_amount, deviation_bps);
    
    // Add to pool
    let new_pool = current_pool.saturating_add(recovered);
    storage::set_mev_pool(env, new_pool);
    
    // Credit depositor
    let claimed = current_claimed.saturating_add(recovered);
    storage::set_mev_claimed(env, depositor, claimed);
}
```

## Common Rounding Cases

| Amount | Deviation | Result | Note |
|--------|-----------|--------|------|
| 1000 | 100 | 10 | Exact (1%) |
| 1000 | 200 | 20 | Exact (2%, threshold) |
| 100 | 50 | 0 | Truncates (0.5% < 1 unit) |
| 333 | 100 | 3 | Truncates (3.33 → 3) |
| 1 | 10_000 | 1 | Maximum (100%) |
| 1e15 | 100 | 1e13 | At scale (1%, max deposit) |

## Verification Checklist

Before deployment:

- [ ] Formula matches specification: `amount × (deviation_bps / 10_000)`
- [ ] Precision maintained for small amounts (1-100 units)
- [ ] Precision maintained for large amounts (1e14-1e15)
- [ ] Overflow protection in place (`checked_mul`)
- [ ] Rounding behavior documented (truncation, never over-recover)
- [ ] All 48 tests pass
- [ ] Real-world scenarios validated (sandwich, flash loan)
- [ ] Integration with contract.rs complete
- [ ] Gas profiling acceptable

## Testing

```bash
cd /workspaces/SAFE-HAVEN/contracts/safe-haven
cargo test --lib mev_recovery_precision
```

**Expected:** `48 passed`

## Precision vs Price-Difference Method

### ❌ Old Method (Incorrect)
```rust
let mev_amount = (current_price - twap).abs();
// Problem: Returns price difference, not scaled recovery
// Not in token units, varies with price scale
```

### ✅ New Method (Correct)
```rust
let mev_amount = calculate_mev_recovered(deposit_amount, deviation_bps);
// Correct: Returns recovery scaled to deposit amount
// In token units, consistent across any decimal precision
```

## Rounding Acceptability

**Rounding Loss:** ≤ 9,999 units (smallest token unit)

**Examples:**
- USDC: ≤ 0.009999 USD (< 1¢)
- DAI: ≤ 1e-14 USD (negligible)
- WBTC: ≤ 0.00009999 BTC (negligible)

**Verdict:** ✅ Acceptable — below standard financial precision

## Questions?

See full verification report: `MEV_RECOVERY_PRECISION_VERIFICATION.md`

---

**Version:** 1.0  
**Last Updated:** 2024-11-23
