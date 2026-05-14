use scrypto::prelude::*;

// ===== TIER THRESHOLDS =====
// Voting threshold: minimum rFOAF to vote on a tier
// Proposal threshold: minimum rFOAF to submit (10x voting threshold)
pub const TIER_1_VOTE: Decimal = dec!("10");
pub const TIER_1_SUBMIT: Decimal = dec!("100");
pub const TIER_2_VOTE: Decimal = dec!("100");
pub const TIER_2_SUBMIT: Decimal = dec!("1000");
pub const TIER_3_VOTE: Decimal = dec!("1000");
pub const TIER_3_SUBMIT: Decimal = dec!("10000");
pub const TIER_4_VOTE: Decimal = dec!("10000");
pub const TIER_4_SUBMIT: Decimal = dec!("100000");

// Quorum: max(absolute_floor, qualifying_count * percentage_floor)
pub const TIER_1_ABS_FLOOR: u64 = 3;
pub const TIER_2_ABS_FLOOR: u64 = 5;
pub const TIER_3_ABS_FLOOR: u64 = 7;
pub const TIER_4_ABS_FLOOR: u64 = 10;

pub const TIER_1_PCT_FLOOR: Decimal = dec!("0.05");
pub const TIER_2_PCT_FLOOR: Decimal = dec!("0.05");
pub const TIER_3_PCT_FLOOR: Decimal = dec!("0.10");
pub const TIER_4_PCT_FLOOR: Decimal = dec!("0.15");

#[derive(ScryptoSbor, Clone, Debug, PartialEq)]
pub enum ProposalTier {
    Tier1, // community programs, grants
    Tier2, // parameter changes
    Tier3, // treasury, fee structure
    Tier4, // constitutional, upgrades
}

impl ProposalTier {
    pub fn voting_threshold(&self) -> Decimal {
        match self { Self::Tier1 => TIER_1_VOTE, Self::Tier2 => TIER_2_VOTE,
                     Self::Tier3 => TIER_3_VOTE, Self::Tier4 => TIER_4_VOTE }
    }
    pub fn proposal_threshold(&self) -> Decimal {
        match self { Self::Tier1 => TIER_1_SUBMIT, Self::Tier2 => TIER_2_SUBMIT,
                     Self::Tier3 => TIER_3_SUBMIT, Self::Tier4 => TIER_4_SUBMIT }
    }
    pub fn pass_threshold(&self) -> Decimal {
        match self { Self::Tier1 | Self::Tier2 => dec!("0.50"),
                     Self::Tier3 | Self::Tier4 => dec!("0.66") }
    }
    pub fn abs_floor(&self) -> u64 {
        match self { Self::Tier1 => TIER_1_ABS_FLOOR, Self::Tier2 => TIER_2_ABS_FLOOR,
                     Self::Tier3 => TIER_3_ABS_FLOOR, Self::Tier4 => TIER_4_ABS_FLOOR }
    }
    pub fn pct_floor(&self) -> Decimal {
        match self { Self::Tier1 => TIER_1_PCT_FLOOR, Self::Tier2 => TIER_2_PCT_FLOOR,
                     Self::Tier3 => TIER_3_PCT_FLOOR, Self::Tier4 => TIER_4_PCT_FLOOR }
    }
    pub fn tier_index(&self) -> u8 {
        match self { Self::Tier1 => 1, Self::Tier2 => 2, Self::Tier3 => 3, Self::Tier4 => 4 }
    }
    pub fn voting_duration(&self) -> u64 {
        match self { Self::Tier1 => 1008, Self::Tier2 => 2016,
                     Self::Tier3 => 4032, Self::Tier4 => 8064 }
    }
}

#[derive(ScryptoSbor, Clone, Debug, PartialEq)]
pub enum ProposalStatus { Active, Passed, Failed, Executed }

/// On-chain executable action attached to a proposal.
/// Initially supports treasury disbursement; extend as governance scope grows.
#[derive(ScryptoSbor, Clone, Debug)]
pub enum ExecutionAction {
    /// No on-chain effect. Use for signaling, statements of intent, etc.
    Signal,
    /// Disburse FOAF from the treasury to a specific recipient.
    TreasuryDisburse { amount: Decimal, recipient: ComponentAddress },
    /// Release the treasury emergency lock. Requires governance role on treasury.
    TreasuryEmergencyUnlock,
}

#[derive(ScryptoSbor, Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub tier: ProposalTier,
    pub proposer: NonFungibleLocalId,
    pub created_epoch: u64,
    pub voting_end_epoch: u64,
    pub votes_for: u64,
    pub votes_against: u64,
    pub status: ProposalStatus,
    pub action: ExecutionAction,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ProposalCreatedEvent { pub id: u64, pub tier: ProposalTier, pub epoch: u64 }

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct VoteCastEvent {
    pub proposal_id: u64,
    pub voter_id: NonFungibleLocalId,
    pub vote_for: bool,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ProposalExecutedEvent {
    pub proposal_id: u64, pub passed: bool,
    pub votes_for: u64, pub votes_against: u64,
    pub quorum_required: u64,
}

#[blueprint]
#[events(ProposalCreatedEvent, VoteCastEvent, ProposalExecutedEvent)]
mod governance_module {
    enable_method_auth! {
        roles {
            admin => updatable_by: [OWNER];
            council => updatable_by: [OWNER];
        },
        methods {
            submit_proposal  => PUBLIC;
            vote             => PUBLIC;
            execute_proposal => PUBLIC;
            get_proposal     => PUBLIC;
            list_proposals   => PUBLIC;
            set_treasury     => restrict_to: [admin];
            // set_staking deliberately omitted: rebinding the rFOAF oracle post-hoc
            // would let admin rewrite vote weights for in-flight proposals. The staking
            // component address is fixed at instantiate-time. If migration is ever needed,
            // it has to flow through a governance proposal that redeploys this component.
        }
    }

    struct FoafGovernance {
        /// Staking component — source of truth for rFOAF balances and tier counts.
        /// Fixed at instantiate-time; cannot be changed without redeploy.
        staking_component: ComponentAddress,

        proposals: KeyValueStore<u64, Proposal>,
        proposal_count: u64,

        /// Votes are keyed by (proposal_id, voter_local_id) using the typed local_id
        /// directly. Previous version stringified the id, which was fragile against
        /// Scrypto formatting changes.
        votes: KeyValueStore<(u64, NonFungibleLocalId), bool>,

        admin_badge: ResourceAddress,
        council_badge: ResourceAddress,
        treasury_component: Option<ComponentAddress>,
    }

    impl FoafGovernance {
        pub fn instantiate(
            staking_component: ComponentAddress,
            admin_badge: ResourceAddress,
            council_badge: ResourceAddress,
        ) -> Global<FoafGovernance> {
            let (address_reservation, _) =
                Runtime::allocate_component_address(FoafGovernance::blueprint_id());
            Self {
                staking_component,
                proposals: KeyValueStore::new(),
                proposal_count: 0,
                votes: KeyValueStore::new(),
                admin_badge,
                council_badge,
                treasury_component: None,
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(admin_badge))))
            .with_address(address_reservation)
            .roles(roles! {
                admin => rule!(require(admin_badge));
                council => rule!(require(council_badge));
            })
            .globalize()
        }

        /// Submit a proposal.
        /// Requires: voter badge proof (identity) + rFOAF proof >= tier.proposal_threshold()
        fn get_voter_badge_res(&self) -> ResourceAddress {
            let raw: Vec<u8> = ScryptoVmV1Api::object_call(
                self.staking_component.as_node_id(),
                "voter_badge_resource",
                scrypto_args!(),
            );
            scrypto_decode::<ResourceAddress>(&raw).unwrap()
        }

        /// Fetch rFOAF balance for a voter from staking component
        /// No rFOAF proof needed — rFOAF lives in staking vault, not user account
        fn get_rfoaf_balance(&self, voter_id: &NonFungibleLocalId) -> Decimal {
            let raw: Vec<u8> = ScryptoVmV1Api::object_call(
                self.staking_component.as_node_id(),
                "get_rfoaf_balance_for_voter",
                scrypto_args!(voter_id.clone()),
            );
            scrypto_decode::<Decimal>(&raw).unwrap()
        }

        pub fn submit_proposal(
            &mut self,
            title: String,
            description: String,
            tier: ProposalTier,
            action: ExecutionAction,
            voter_badge_proof: Proof,
        ) -> u64 {
            let vbr = self.get_voter_badge_res();
            let badge_checked = voter_badge_proof.check(vbr);
            let voter_ids = badge_checked.as_non_fungible().non_fungible_local_ids();
            assert!(voter_ids.len() == 1, "Must provide exactly one voter badge");
            let voter_id = voter_ids.into_iter().next().unwrap();

            // Lookup rFOAF balance via staking component — no proof needed
            // rFOAF lives in staking vault, user cannot produce a proof of it
            let rfoaf_balance = self.get_rfoaf_balance(&voter_id);
            assert!(
                rfoaf_balance >= tier.proposal_threshold(),
                "Insufficient rFOAF to submit. Required: {}, held: {}",
                tier.proposal_threshold(), rfoaf_balance
            );

            // Validate that the action is consistent with the tier. Treasury and
            // emergency actions require Tier 3+; signaling is open to any tier.
            match &action {
                ExecutionAction::Signal => {}
                ExecutionAction::TreasuryDisburse { .. }
                | ExecutionAction::TreasuryEmergencyUnlock => {
                    assert!(
                        matches!(tier, ProposalTier::Tier3 | ProposalTier::Tier4),
                        "Treasury actions require Tier 3 or Tier 4"
                    );
                }
            }

            let current_epoch = Runtime::current_epoch().number();
            let id = self.proposal_count;
            self.proposal_count += 1;

            self.proposals.insert(id, Proposal {
                id,
                title,
                description,
                tier: tier.clone(),
                proposer: voter_id,
                created_epoch: current_epoch,
                voting_end_epoch: current_epoch + tier.voting_duration(),
                votes_for: 0,
                votes_against: 0,
                status: ProposalStatus::Active,
                action,
            });

            Runtime::emit_event(ProposalCreatedEvent { id, tier, epoch: current_epoch });
            id
        }

        pub fn vote(
            &mut self,
            proposal_id: u64,
            vote_for: bool,
            voter_badge_proof: Proof,
        ) {
            let vbr = self.get_voter_badge_res();
            let badge_checked = voter_badge_proof.check(vbr);
            let voter_ids = badge_checked.as_non_fungible().non_fungible_local_ids();
            assert!(voter_ids.len() == 1, "Must provide exactly one voter badge");
            let voter_id = voter_ids.into_iter().next().unwrap();

            // Lookup rFOAF balance via staking — no proof needed
            let rfoaf_balance = self.get_rfoaf_balance(&voter_id);

            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");

            assert!(proposal.status == ProposalStatus::Active, "Proposal is not active");
            assert!(
                Runtime::current_epoch().number() <= proposal.voting_end_epoch,
                "Voting period has ended"
            );
            assert!(
                rfoaf_balance >= proposal.tier.voting_threshold(),
                "Insufficient rFOAF to vote. Required: {}, held: {}",
                proposal.tier.voting_threshold(), rfoaf_balance
            );

            let vote_key = (proposal_id, voter_id.clone());
            assert!(self.votes.get(&vote_key).is_none(), "Already voted on this proposal");

            if vote_for { proposal.votes_for += 1; } else { proposal.votes_against += 1; }
            self.votes.insert(vote_key, true);

            Runtime::emit_event(VoteCastEvent { proposal_id, voter_id, vote_for });
        }

        pub fn execute_proposal(&mut self, proposal_id: u64) {
            let current_epoch = Runtime::current_epoch().number();

            // Read-only scope: decide pass/fail without holding a mutable borrow
            // across the cross-component execution call below.
            let (passed, quorum_required, action, tier_idx) = {
                let proposal = self.proposals.get(&proposal_id)
                    .expect("Proposal not found");
                assert!(proposal.status == ProposalStatus::Active, "Proposal already processed");
                assert!(current_epoch > proposal.voting_end_epoch, "Voting period has not ended");

                let total_votes = proposal.votes_for + proposal.votes_against;
                let tier_index = proposal.tier.tier_index();
                let qualifying_raw: Vec<u8> = ScryptoVmV1Api::object_call(
                    self.staking_component.as_node_id(),
                    "qualifying_count",
                    scrypto_args!(tier_index),
                );
                let qualifying: u64 = scrypto_decode(&qualifying_raw)
                    .expect("Failed to decode qualifying_count");

                // Scale-aware quorum: max(absolute_floor, count * pct_floor)
                let pct_quorum = Decimal::from(qualifying) * proposal.tier.pct_floor();
                let pct_quorum_u64 = decimal_floor_to_u64(pct_quorum);
                let quorum_required = proposal.tier.abs_floor().max(pct_quorum_u64);

                let quorum_met = total_votes >= quorum_required;
                let passed = quorum_met && total_votes > 0 && {
                    let ratio = Decimal::from(proposal.votes_for) / Decimal::from(total_votes);
                    ratio >= proposal.tier.pass_threshold()
                };

                (passed, quorum_required, proposal.action.clone(), tier_index)
            };

            // Perform the on-chain action only if the proposal passed.
            // For Signal proposals there is no on-chain effect; the status change is
            // the only outcome.
            let executed = if passed {
                self.perform_action(&action);
                true
            } else { false };

            // Write back final status. Passed-and-executed proposals become Executed;
            // passed-but-no-op (Signal) proposals become Passed; failures Failed.
            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");
            proposal.status = match (passed, executed, &action) {
                (true, true, ExecutionAction::Signal) => ProposalStatus::Passed,
                (true, true, _) => ProposalStatus::Executed,
                (true, false, _) => ProposalStatus::Passed,
                (false, _, _) => ProposalStatus::Failed,
            };

            Runtime::emit_event(ProposalExecutedEvent {
                proposal_id, passed,
                votes_for: proposal.votes_for,
                votes_against: proposal.votes_against,
                quorum_required,
            });

            // tier_idx kept around for potential future per-tier execution telemetry
            let _ = tier_idx;
        }

        /// Internal: dispatch the proposal's action via the appropriate downstream
        /// component. Governance holds the council badge that authorizes treasury calls.
        fn perform_action(&self, action: &ExecutionAction) {
            match action {
                ExecutionAction::Signal => {}
                ExecutionAction::TreasuryDisburse { amount, recipient } => {
                    let treasury = self.treasury_component
                        .expect("Treasury not wired");
                    let _raw: Vec<u8> = ScryptoVmV1Api::object_call(
                        treasury.as_node_id(),
                        "disburse",
                        scrypto_args!(*amount, *recipient),
                    );
                }
                ExecutionAction::TreasuryEmergencyUnlock => {
                    let treasury = self.treasury_component
                        .expect("Treasury not wired");
                    let _raw: Vec<u8> = ScryptoVmV1Api::object_call(
                        treasury.as_node_id(),
                        "emergency_unlock",
                        scrypto_args!(),
                    );
                }
            }
        }

        pub fn get_proposal(&self, id: u64) -> Proposal {
            self.proposals.get(&id).map(|p| p.clone()).expect("Proposal does not exist")
        }

        pub fn list_proposals(&self, from: u64, count: u64) -> Vec<Proposal> {
            (from..from + count)
                .filter_map(|i| self.proposals.get(&i).map(|p| p.clone()))
                .collect()
        }

        pub fn set_treasury(&mut self, treasury: ComponentAddress) {
            assert!(self.treasury_component.is_none(),
                "Treasury already wired; cannot rebind");
            self.treasury_component = Some(treasury);
        }
    }
}

/// Convert a Decimal to a u64 by flooring, returning 0 on overflow or negative input.
/// Bounded through i128 to catch values outside u64 range cleanly.
fn decimal_floor_to_u64(d: Decimal) -> u64 {
    match d.checked_floor() {
        Some(floored) => floored
            .to_string()
            .parse::<i128>()
            .ok()
            .and_then(|i| u64::try_from(i.max(0)).ok())
            .unwrap_or(0),
        None => 0,
    }
}
