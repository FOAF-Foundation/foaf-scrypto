# governance/

Governance components. How the rules of the protocol can be changed.

These are meta-protocol: about the protocol, not part of the rules of interaction themselves. Conceptually distinct from the `../protocol/` components (which implement the rules) and from `../treasury/` (which holds reserves and only disburses when governance approves).

## Components, to be sketched

- **Badges as authority resources**. Each governance tier corresponds to a badge type. Holding a tier-N badge qualifies an account to vote in tier-N proposals. Badges are issued and revoked based on rFOAF stake and lock duration.
- **Proposal component**. Submission, discussion period, voting period, quorum check, execution. Execution calls into the relevant target component (parameter change, treasury disbursement) directly rather than requiring a multisig step.
- **Voting components**. Threshold-tiered: rFOAF stake and lock duration determine which tiers a holder qualifies for, but vote weight within a tier is one-per-account, not stake-weighted within a tier.

## Design status

Direction has shifted from the original stake-weighted design with rFOAF revenue-share to threshold-tiered voting with governance-tier qualification (no direct revenue share). The shift is partly to reduce Howey exposure and partly to align with cooperative principles (operational capacity scales with stake; governance weight does not).

For the current direction, see [tokenomics.md](https://docs.foaf.foundation/about/tokenomics/) in the foaf-docs repo. If `governance.md` and `tokenomics.md` ever conflict, `tokenomics.md` is authoritative until the design is finalized.
