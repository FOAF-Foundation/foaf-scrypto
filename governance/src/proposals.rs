use scrypto::prelude::*;

/// Governance tier thresholds (rFOAF required to qualify)
/// Tier 0: any FOAF holder (signaling only)
/// Tier 1: 10+ rFOAF  — community programs, grants
/// Tier 2: 100+ rFOAF — parameter changes
/// Tier 3: 1,000+ rFOAF — treasury, fee structure
/// Tier 4: 10,000+ rFOAF — constitutional, upgrades
pub const TIER_1_THRESHOLD: Decimal = dec!("10");
pub const TIER_2_THRESHOLD: Decimal = dec!("100");
pub const TIER_3_THRESHOLD: Decimal = dec!("1000");
pub const TIER_4_THRESHOLD: Decimal = dec!("10000");

/// One vote per qualifying tier per account.
/// Stake determines which tiers you qualify for,
/// NOT vote weight within a tier.
#[derive(ScryptoSbor, Clone, Debug, PartialEq)]
pub enum ProposalTier {
    Tier1, // community programs, grants
    Tier2, // parameter changes
    Tier3, // treasury, fee structure
    Tier4, // constitutional, upgrades
}

impl ProposalTier {
    /// Minimum rFOAF required to vote on this tier
    pub fn voting_threshold(&self) -> Decimal {
        match self {
            ProposalTier::Tier1 => TIER_1_THRESHOLD,
            ProposalTier::Tier2 => TIER_2_THRESHOLD,
            ProposalTier::Tier3 => TIER_3_THRESHOLD,
            ProposalTier::Tier4 => TIER_4_THRESHOLD,
        }
    }

    /// Minimum rFOAF required to submit a proposal of this tier
    pub fn proposal_threshold(&self) -> Decimal {
        match self {
            ProposalTier::Tier1 => TIER_1_THRESHOLD,
            ProposalTier::Tier2 => TIER_2_THRESHOLD,
            ProposalTier::Tier3 => TIER_3_THRESHOLD,
            ProposalTier::Tier4 => TIER_4_THRESHOLD,
        }
    }

    /// Pass threshold: fraction of votes_for / (votes_for + votes_against)
    pub fn pass_threshold(&self) -> Decimal {
        match self {
            ProposalTier::Tier1 => dec!("0.50"), // simple majority
            ProposalTier::Tier2 => dec!("0.50"),
            ProposalTier::Tier3 => dec!("0.66"), // supermajority
            ProposalTier::Tier4 => dec!("0.66"),
        }
    }

    /// Minimum number of qualifying voters required (quorum)
    pub fn quorum(&self) -> u64 {
        match self {
            ProposalTier::Tier1 => 3,  // low bar for community signals
            ProposalTier::Tier2 => 5,
            ProposalTier::Tier3 => 7,
            ProposalTier::Tier4 => 10,
        }
    }

    /// Voting duration in epochs
    pub fn voting_duration(&self) -> u64 {
        match self {
            ProposalTier::Tier1 => 1008,  // ~3.5 days
            ProposalTier::Tier2 => 2016,  // ~1 week
            ProposalTier::Tier3 => 4032,  // ~2 weeks
            ProposalTier::Tier4 => 8064,  // ~4 weeks
        }
    }
}

#[derive(ScryptoSbor, Clone, Debug, PartialEq)]
pub enum ProposalStatus {
    Active,
    Passed,
    Failed,
    Executed,
}

#[derive(ScryptoSbor, Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub tier: ProposalTier,
    pub proposer: ComponentAddress,
    pub created_epoch: u64,
    pub voting_end_epoch: u64,
    /// votes_for: count of accounts that voted in favour (1 per qualifying account)
    pub votes_for: u64,
    /// votes_against: count of accounts that voted against
    pub votes_against: u64,
    pub status: ProposalStatus,
    /// Track voters to prevent double-voting
    pub voters: Vec<ComponentAddress>,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ProposalCreatedEvent {
    pub id: u64,
    pub tier: ProposalTier,
    pub epoch: u64,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct VoteCastEvent {
    pub proposal_id: u64,
    pub voter: ComponentAddress,
    pub vote_for: bool,
    pub tier_qualified: ProposalTier,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ProposalExecutedEvent {
    pub proposal_id: u64,
    pub passed: bool,
    pub votes_for: u64,
    pub votes_against: u64,
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
            submit_proposal      => PUBLIC;
            vote                 => PUBLIC;
            execute_proposal     => PUBLIC;
            get_proposal         => PUBLIC;
            list_proposals       => PUBLIC;
            get_qualifying_tiers => PUBLIC;
            set_treasury         => restrict_to: [admin];
        }
    }

    struct FoafGovernance {
        /// rFOAF resource address — used to check tier qualification
        rfoaf_resource: ResourceAddress,

        proposals: KeyValueStore<u64, Proposal>,
        proposal_count: u64,

        admin_badge: ResourceAddress,
        council_badge: ResourceAddress,

        /// Optional treasury component for Tier 3+ disbursement execution
        treasury_component: Option<ComponentAddress>,
    }

    impl FoafGovernance {
        pub fn instantiate(
            rfoaf_resource: ResourceAddress,
            admin_badge: ResourceAddress,
            council_badge: ResourceAddress,
        ) -> Global<FoafGovernance> {
            let (address_reservation, _) =
                Runtime::allocate_component_address(FoafGovernance::blueprint_id());

            Self {
                rfoaf_resource,
                proposals: KeyValueStore::new(),
                proposal_count: 0,
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

        /// Submit a proposal. Caller must hold >= tier.proposal_threshold() rFOAF.
        /// One vote per qualifying account — not stake-weighted within a tier.
        pub fn submit_proposal(
            &mut self,
            title: String,
            description: String,
            tier: ProposalTier,
            rfoaf_proof: Proof,
            caller: ComponentAddress,
        ) -> u64 {
            let checked = rfoaf_proof.check(self.rfoaf_resource);
            let rfoaf_balance = checked.amount();

            assert!(
                rfoaf_balance >= tier.proposal_threshold(),
                "Insufficient rFOAF to submit Tier proposal. Required: {}, held: {}",
                tier.proposal_threshold(),
                rfoaf_balance
            );

            let current_epoch = Runtime::current_epoch().number();
            let voting_end_epoch = current_epoch + tier.voting_duration();
            let id = self.proposal_count;
            self.proposal_count += 1;

            self.proposals.insert(id, Proposal {
                id,
                title,
                description,
                tier: tier.clone(),
                proposer: caller,
                created_epoch: current_epoch,
                voting_end_epoch,
                votes_for: 0,
                votes_against: 0,
                status: ProposalStatus::Active,
                voters: Vec::new(),
            });

            Runtime::emit_event(ProposalCreatedEvent { id, tier, epoch: current_epoch });
            id
        }

        /// Cast a vote. Each account gets exactly ONE vote per proposal
        /// if they hold >= tier.voting_threshold() rFOAF.
        /// Vote weight is always 1 — not stake-weighted.
        pub fn vote(
            &mut self,
            proposal_id: u64,
            vote_for: bool,
            rfoaf_proof: Proof,
            caller: ComponentAddress,
        ) {
            let checked = rfoaf_proof.check(self.rfoaf_resource);
            let rfoaf_balance = checked.amount();

            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");

            assert!(
                proposal.status == ProposalStatus::Active,
                "Proposal is not active"
            );
            assert!(
                Runtime::current_epoch().number() <= proposal.voting_end_epoch,
                "Voting period has ended"
            );
            assert!(
                !proposal.voters.contains(&caller),
                "Already voted on this proposal"
            );

            // Check tier qualification — must hold >= tier threshold
            assert!(
                rfoaf_balance >= proposal.tier.voting_threshold(),
                "Insufficient rFOAF to vote on this tier. Required: {}, held: {}",
                proposal.tier.voting_threshold(),
                rfoaf_balance
            );

            // ONE vote per qualifying account — not stake-weighted
            if vote_for {
                proposal.votes_for += 1;
            } else {
                proposal.votes_against += 1;
            }
            proposal.voters.push(caller);

            Runtime::emit_event(VoteCastEvent {
                proposal_id,
                voter: caller,
                vote_for,
                tier_qualified: proposal.tier.clone(),
            });
        }

        /// Finalise a proposal after voting ends.
        /// Checks quorum and pass threshold per tier.
        pub fn execute_proposal(&mut self, proposal_id: u64) {
            let current_epoch = Runtime::current_epoch().number();
            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");

            assert!(
                proposal.status == ProposalStatus::Active,
                "Proposal already processed"
            );
            assert!(
                current_epoch > proposal.voting_end_epoch,
                "Voting period has not ended"
            );

            let total_votes = proposal.votes_for + proposal.votes_against;
            let quorum_met = total_votes >= proposal.tier.quorum();

            let passed = quorum_met && {
                let ratio = Decimal::from(proposal.votes_for)
                    / Decimal::from(total_votes);
                ratio >= proposal.tier.pass_threshold()
            };

            proposal.status = if passed {
                ProposalStatus::Passed
            } else {
                ProposalStatus::Failed
            };

            Runtime::emit_event(ProposalExecutedEvent {
                proposal_id,
                passed,
                votes_for: proposal.votes_for,
                votes_against: proposal.votes_against,
            });
        }

        /// View: which tiers does an account qualify for given their rFOAF balance
        pub fn get_qualifying_tiers(&self, rfoaf_proof: Proof) -> Vec<ProposalTier> {
            let balance = rfoaf_proof.check(self.rfoaf_resource).amount();
            let mut tiers = Vec::new();
            if balance >= TIER_1_THRESHOLD { tiers.push(ProposalTier::Tier1); }
            if balance >= TIER_2_THRESHOLD { tiers.push(ProposalTier::Tier2); }
            if balance >= TIER_3_THRESHOLD { tiers.push(ProposalTier::Tier3); }
            if balance >= TIER_4_THRESHOLD { tiers.push(ProposalTier::Tier4); }
            tiers
        }

        pub fn get_proposal(&self, id: u64) -> Proposal {
            self.proposals.get(&id).map(|p| p.clone())
                .expect("Proposal does not exist")
        }

        pub fn list_proposals(&self, from: u64, count: u64) -> Vec<Proposal> {
            (from..from + count)
                .filter_map(|i| self.proposals.get(&i).map(|p| p.clone()))
                .collect()
        }

        pub fn set_treasury(&mut self, treasury: ComponentAddress) {
            self.treasury_component = Some(treasury);
        }
    }
}
