# foaf-scrypto

On-chain components for the FOAF protocol on Radix Babylon. Scrypto code covering the token, staking, RHEO accrual and consumption, badge-gated governance, and treasury.

## What's in here

- `protocol/` — Token, staking, RHEO, settlement primitives. The rules of interaction.
- `governance/` — Proposals, voting, badges. How the rules of the protocol get changed.
- `treasury/` — Foundation reserve, proposal-gated disbursement.

This is the on-chain layer of the FOAF stack. The off-chain trustline / credit-graph layer lives in [foaf-protocol-ruby](https://github.com/FOAF-Foundation/foaf-protocol-ruby). The Hyperlane bridge that brings FOAF from Ethereum will live in `foaf-bridge` (forthcoming).

## Background

For the broader architectural context:

- [Protocol vision](https://docs.foaf.foundation/technical/protocol-vision/) — long-form architecture direction
- [Scrypto protocol stack](https://docs.foaf.foundation/technical/scrypto-protocol/) — scope and design intent
- [Tokenomics](https://docs.foaf.foundation/about/tokenomics/) — token economics, RHEO accrual model
- [Governance](https://docs.foaf.foundation/about/governance/) — threshold-tier design

## Status

Pre-mainnet. The spec is partly set, partly being refined as we sketch initial components on Stokenet. The spec freezes when the design has stabilized through code.

The Hyperlane bridge that puts FOAF on Radix is upstream of mainnet deployment. Stokenet work uses a mock FOAF resource matching the agreed shape (25M total, indivisible, fixed).

## Development

Design questions and architecture conversations happen in this repo's Issues. The intent is for the spec to emerge from code rather than the other way around: sketch a component, hit edge cases, decide, document. Spec freezes after the design has stress-tested itself through real Scrypto.

## Security

See [SECURITY.md](./SECURITY.md) for vulnerability reporting.

## License

MIT
