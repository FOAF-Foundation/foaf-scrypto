# Security policy

## Reporting a vulnerability

If you find a security issue in the FOAF on-chain components, please report it privately rather than opening a public issue. Use GitHub's private security advisory feature:

https://github.com/FOAF-Foundation/foaf-scrypto/security/advisories/new

We will acknowledge receipt within 48 hours and aim to triage within a week.

## Scope

In scope:
- Issues in the Scrypto components in this repository
- Issues in deployment configuration that could affect mainnet operations

Out of scope:
- Issues in the underlying Radix protocol (report to Radix Foundation)
- Issues in third-party dependencies (report upstream)
- Bugs that don't have security implications (open a public issue instead)

## Known limitations (pre-mainnet)

### stake_vfoaf / stake_rfoaf: user-supplied account parameter

`stake_vfoaf(foaf_bucket, account)` and `stake_rfoaf(foaf_bucket, lock, account)`
accept `account: ComponentAddress` as a user-supplied parameter.

`Runtime::global_caller()` is not available in Scrypto v1.3, so caller identity
cannot be derived from the runtime at stake time.

**Impact**: griefing only, not theft. An attacker can credit someone else's
stake list with their own FOAF (they lose their own tokens, the victim gains
an unexpected position). No FOAF is stolen.

**Planned fix**: upgrade to Scrypto version that exposes end-caller identity,
or require an account-owner proof at stake time.

### unstake_vfoaf: receipt NFT not burned post-unstake

The vFOAF stake receipt NFT remains in the user's account after unstake.
Replay is blocked by `used_vstake_receipts: KeyValueStore<NonFungibleLocalId, bool>`.
Physical burn of the receipt requires a worktop bucket, deferred to Stokenet
manifest layer.

**Impact**: cosmetic only. The receipt cannot be used to unstake again.

## Responsible disclosure

We do not currently offer a bug bounty. Reporters who follow responsible
disclosure will be publicly credited in release notes if they wish.
