# protocol/

Protocol primitives. The rules of how participants interact with the FOAF system on chain.

These components are conceptually distinct from the governance components in `../governance/` (which determine how these rules can be changed) and from the treasury in `../treasury/` (which holds Foundation reserves and disburses on proposal execution).

## Components, to be sketched

- **FOAF resource**. The token itself. The actual resource is bridged in via the Hyperlane warp route (see `foaf-bridge`); this directory may hold a mock for Stokenet work and any additional resource-level metadata or behaviour we add on Radix.
- **vFOAF staking component**. Liquid stake. Stake FOAF, receive vFOAF (soulbound) at 1:1, unstake any time. Provides standard RHEO generation and basic voting eligibility.
- **rFOAF staking component**. Time-locked stake. 6 months to 2+ years, with a multiplier curve. Provides enhanced RHEO generation and qualifies the holder for higher governance tiers.
- **RHEO accrual model**. Lazy: balance computed on demand from `stake × elapsed × multiplier`. Mint-and-burn happen in the same manifest as the operation that consumes RHEO. RHEO is non-transferable and consumed through trust-graph chains.
- **Optional on-chain settlement path**. For trustlines that opt into chain proof. Default trustlines stay bilateral and off-chain; this path is invoked only when a specific trustline needs full on-chain proof (high-value trades, escrow, regulatory contexts).

## Design status

Mid-revision. Sketching on Stokenet first. The component-level spec (interfaces, function signatures, manifest structures) is what gets developed here. Spec freezes only after the design has stress-tested itself through code.
