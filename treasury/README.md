# treasury/

DAO-controlled treasury component.

Holds the Foundation's FOAF reserve. Disburses only on successful proposal execution from the governance layer. Designed so no external signing authority is required: execution flows from a passed proposal directly to a component method call to a disbursement, all on-chain and atomic.

## Components, to be sketched

- **Treasury vault**. Holds the 12% Treasury & Operations allocation (3M FOAF). Withdrawal is gated on receipt of an authority-bearing badge issued by a successful proposal execution.
- **Disbursement logic**. Proposal-driven: the proposal component calls treasury methods after a vote passes. Recipients, amounts, and conditions are all encoded in the proposal itself.

## Out of scope here

- Monetary policy formula execution lives in `../protocol/` with the RHEO components, not here.
- The on-chain identity of the Foundation as initial holder of the reserve is a deployment concern, not a component concern.
- The fee structure that may eventually feed the treasury (3% transaction fee, 3% multi-hop markup, 1% per-hop routing premium) is also DAO-governed and lives in the protocol layer for routing.

## Design status

Smallest of the three component families. Likely the last to be sketched, since it depends on a working proposal component to be useful.
