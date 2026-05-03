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
            submit_proposal      => PUBLIC;
            vote                 => PUBLIC;
            execute_proposal     => PUBLIC;
            get_proposal         => PUBLIC;
            list_proposals       => PUBLIC;
            get_qualifying_tiers => PUBLIC;
            set_treasury         => restrict_to: [admin];
            set_staking          => restrict_to: [admin];
        }
    }

    struct FoafGovernance {
        /// rFOAF resource address — for tier qualification check
        rfoaf_resource: ResourceAddress,

        /// Voter identity badge resource address


        /// Staking component — for qualifying_count() queries
        staking_component: ComponentAddress,

        proposals: KeyValueStore<u64, Proposal>,
        proposal_count: u64,

        /// voters per proposal: proposal_id -> voter_local_id -> voted
        /// KeyValueStore<(proposal_id, voter_local_id_string), bool>
        votes: KeyValueStore<(u64, String), bool>,

        admin_badge: ResourceAddress,
        council_badge: ResourceAddress,
        treasury_component: Option<ComponentAddress>,
    }

    impl FoafGovernance {
        pub fn instantiate(
            rfoaf_resource: ResourceAddress,
    
            staking_component: ComponentAddress,
            admin_badge: ResourceAddress,
            council_badge: ResourceAddress,
        ) -> Global<FoafGovernance> {
            let (address_reservation, _) =
                Runtime::allocate_component_address(FoafGovernance::blueprint_id());
            Self {
                rfoaf_resource,
    
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

        pub fn submit_proposal(
            &mut self,
            title: String,
            description: String,
            tier: ProposalTier,
            voter_badge_proof: Proof,
            rfoaf_proof: Proof,
        ) -> u64 {
            let vbr = self.get_voter_badge_res();
            let badge_checked = voter_badge_proof.check(vbr);
            let voter_ids = badge_checked.as_non_fungible().non_fungible_local_ids();
            assert!(voter_ids.len() == 1, "Must provide exactly one voter badge");
            let voter_id = voter_ids.into_iter().next().unwrap();

            let rfoaf_balance = rfoaf_proof.check(self.rfoaf_resource).amount();
            assert!(
                rfoaf_balance >= tier.proposal_threshold(),
                "Insufficient rFOAF to submit. Required: {}, held: {}",
                tier.proposal_threshold(), rfoaf_balance
            );

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
            });

            Runtime::emit_event(ProposalCreatedEvent { id, tier, epoch: current_epoch });
            id
        }

        pub fn vote(
            &mut self,
            proposal_id: u64,
            vote_for: bool,
            voter_badge_proof: Proof,
            rfoaf_proof: Proof,
        ) {
            let vbr = self.get_voter_badge_res();
            let badge_checked = voter_badge_proof.check(vbr);
            let voter_ids = badge_checked.as_non_fungible().non_fungible_local_ids();
            assert!(voter_ids.len() == 1, "Must provide exactly one voter badge");
            let voter_id = voter_ids.into_iter().next().unwrap();

            let rfoaf_balance = rfoaf_proof.check(self.rfoaf_resource).amount();

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

            let vote_key = (proposal_id, voter_id.to_string());
            assert!(self.votes.get(&vote_key).is_none(), "Already voted on this proposal");

            if vote_for { proposal.votes_for += 1; } else { proposal.votes_against += 1; }
            self.votes.insert(vote_key, true);

            Runtime::emit_event(VoteCastEvent { proposal_id, voter_id, vote_for });
        }

        pub fn execute_proposal(&mut self, proposal_id: u64) {
            let current_epoch = Runtime::current_epoch().number();
            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");

            assert!(proposal.status == ProposalStatus::Active, "Proposal already processed");
            assert!(current_epoch > proposal.voting_end_epoch, "Voting period has not ended");

            let total_votes = proposal.votes_for + proposal.votes_against;

            // Query qualifying_count from staking component
            let tier_index = proposal.tier.tier_index();
            let qualifying_raw: Vec<u8> = ScryptoVmV1Api::object_call(
                self.staking_component.as_node_id(),
                "qualifying_count",
                scrypto_args!(tier_index),
            );
            let qualifying: u64 = scrypto_decode(&qualifying_raw).expect("Failed to decode qualifying_count");

            // Scale-aware quorum: max(absolute_floor, count * pct_floor)
            let pct_quorum = Decimal::from(qualifying) * proposal.tier.pct_floor();
            let pct_quorum_u64 = pct_quorum.to_string().parse::<u64>().unwrap_or(0);
            let quorum_required = proposal.tier.abs_floor().max(pct_quorum_u64);

            let quorum_met = total_votes >= quorum_required;
            let passed = quorum_met && total_votes > 0 && {
                let ratio = Decimal::from(proposal.votes_for) / Decimal::from(total_votes);
                ratio >= proposal.tier.pass_threshold()
            };

            proposal.status = if passed { ProposalStatus::Passed } else { ProposalStatus::Failed };

            Runtime::emit_event(ProposalExecutedEvent {
                proposal_id, passed,
                votes_for: proposal.votes_for,
                votes_against: proposal.votes_against,
                quorum_required,
            });
        }

        /// View: which tiers does an account qualify for given rFOAF proof
        pub fn get_qualifying_tiers(&self, rfoaf_proof: Proof) -> Vec<ProposalTier> {
            let balance = rfoaf_proof.check(self.rfoaf_resource).amount();
            let mut tiers = Vec::new();
            if balance >= TIER_1_VOTE { tiers.push(ProposalTier::Tier1); }
            if balance >= TIER_2_VOTE { tiers.push(ProposalTier::Tier2); }
            if balance >= TIER_3_VOTE { tiers.push(ProposalTier::Tier3); }
            if balance >= TIER_4_VOTE { tiers.push(ProposalTier::Tier4); }
            tiers
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
            self.treasury_component = Some(treasury);
        }

        pub fn set_staking(&mut self, staking: ComponentAddress) {
            self.staking_component = staking;
        }
    }
}
