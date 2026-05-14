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

### stake_vfoaf / stake_rfoaf: user-supplied account parameter (mitigated)

`stake_vfoaf(foaf_bucket, account)` and `stake_rfoaf(foaf_bucket, lock, account)`
accept `account: ComponentAddress` as a user-supplied parameter.
`Runtime::global_caller()` is not available in current Scrypto, so caller
identity cannot be derived from the runtime at stake time.

**Original concern (resolved 2026-05-14)**: a third party could call stake
"as Alice" and intercept the resulting voter badge or stake receipt to
hijack Alice's governance identity. This was theft of identity, not just
griefing.

**Mitigation**: voter badges and stake receipts are now deposited atomically
into the named `account` via `Account::try_deposit_or_abort` rather than
returned as free buckets. The caller cannot intercept them. The encoded
account in NFT data and the depositing account are guaranteed to agree.

**Remaining griefing**: anyone can still credit anyone's rFOAF balance and
RHEO accrual by staking on their behalf. The attacker burns their own FOAF
(locked in the protocol vault) and the victim ends up with a position they
didn't ask for. The attacker gains nothing; the victim cannot lose anything.
Closing this fully requires Scrypto runtime caller exposure or an
account-owner proof pattern at stake time.

**Account deposit policy note**: accounts with restrictive deposit rules may
reject the badge deposit, in which case the entire transaction aborts. The
account holder must either run a default-allow deposit policy or add the
staking component to their authorized depositor list before staking.

### Treasury disburse: recipient enforcement (fixed 2026-05-14)

Previously, `disburse(amount, recipient)` returned a free `Bucket` and the
`recipient` parameter was only used in the event log, allowing a governance
caller to redirect funds while emitting a falsified disbursement event.

**Fix**: `disburse` now atomically deposits into the named recipient via
`Account::try_deposit_or_abort`. The function returns nothing; the recipient
encoded in the event log and the actual recipient are guaranteed to agree.

### Treasury emergency lock: unlock authority (fixed 2026-05-14)

Previously, `emergency_lock(lock: bool)` was a single admin-gated toggle,
meaning a compromised admin could lock and then unlock the treasury at
will.

**Fix**: split into two methods. `emergency_lock` is admin-gated (any admin
can pause). `emergency_unlock` is governance-gated (only a passed governance
proposal can release the lock). A compromised admin can pause but cannot
drain.

### Governance staking-component oracle: rebinding (fixed 2026-05-14)

Previously, `set_staking(addr)` allowed admin to swap the staking component
that governance reads rFOAF balances and tier counts from, enabling post-hoc
rewriting of vote weights for in-flight proposals.

**Fix**: `set_staking` removed. Staking component address is fixed at
governance instantiate-time. If migration is ever needed, it must flow
through a governance proposal that redeploys the governance component.

### unstake_vfoaf: receipt NFT not burned post-unstake

The vFOAF stake receipt NFT remains in the user's account after unstake.
Replay is blocked by `used_vstake_receipts: KeyValueStore<NonFungibleLocalId, bool>`.
Physical burn of the receipt requires a worktop bucket, deferred to Stokenet
manifest layer.

**Impact**: cosmetic only. The receipt cannot be used to unstake again.

## Responsible disclosure

We do not currently offer a bug bounty. Reporters who follow responsible
disclosure will be publicly credited in release notes if they wish.
