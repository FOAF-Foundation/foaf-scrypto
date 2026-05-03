use scrypto::prelude::*;

/// RHEO has no stored component or token balance.
/// It exists only momentarily: computed -> consumed -> burned within one manifest.
/// The user-visible RHEO balance is a view derived from staking accrual state.

#[derive(ScryptoSbor, Clone, Debug)]
pub struct RheoBalance {
    pub account: ComponentAddress,
    pub computed_balance: Decimal,
    pub as_of_epoch: u64,
}

/// RHEO accrual formula for a single stake position:
/// RHEO = foaf_amount * elapsed_epochs * base_rate * multiplier
pub fn compute_rheo(
    foaf_amount: Decimal,
    elapsed_epochs: u64,
    base_rate: Decimal,
    multiplier: Decimal,
) -> Decimal {
    foaf_amount * Decimal::from(elapsed_epochs) * base_rate * multiplier
}

/// rFOAF multiplier formula per governance spec:
/// multiplier = 1 + 3 * (lock_duration / max_lock), range [1.0, 4.0]
pub fn compute_rfoaf_multiplier(lock_duration_epochs: u64, max_lock_epochs: u64) -> Decimal {
    let ratio = Decimal::from(lock_duration_epochs) / Decimal::from(max_lock_epochs);
    dec!("1") + dec!("3") * ratio
}

/// Event emitted when RHEO is consumed (mint-and-burn in same manifest)
#[derive(ScryptoSbor, ScryptoEvent)]
pub struct RheoMintBurnEvent {
    pub account: ComponentAddress,
    pub amount: Decimal,
    pub purpose: String, // e.g. "fee", "routing", "governance_vote"
    pub epoch: u64,
}
