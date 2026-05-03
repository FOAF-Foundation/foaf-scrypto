use scrypto::prelude::*;

#[derive(ScryptoSbor, Clone, Debug, PartialEq)]
pub enum ProposalStatus { Active, Passed, Failed, Executed }

#[derive(ScryptoSbor, Clone, Debug, PartialEq)]
pub enum ProposalCategory {
    Standard,   // simple majority, 10% quorum
    HighStakes, // 66%+ threshold, 15% quorum, 48h timelock
    Emergency,  // 51%+, 5-of-7 council + 24h vote
}

#[derive(ScryptoSbor, Clone, Debug)]
pub struct Proposal {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub category: ProposalCategory,
    pub proposer: ComponentAddress,
    pub created_epoch: u64,
    pub voting_end_epoch: u64,
    pub votes_for: Decimal,
    pub votes_against: Decimal,
    pub status: ProposalStatus,
    pub voters: Vec<ComponentAddress>,
}

#[blueprint]
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
            get_voting_power => PUBLIC;
        }
    }

    struct FoafGovernance {
        staking_component: ComponentAddress,
        rfoaf_resource: ResourceAddress,
        proposals: KeyValueStore<u64, Proposal>,
        proposal_count: u64,
        quorum_percentage: Decimal,
        voting_duration_epochs: u64,
        admin_badge: ResourceAddress,
        council_badge: ResourceAddress,
        proposal_threshold: Decimal,
        treasury_component: Option<ComponentAddress>,
    }

    impl FoafGovernance {
        pub fn instantiate(
            staking_component: ComponentAddress,
            rfoaf_resource: ResourceAddress,
            admin_badge: ResourceAddress,
            council_badge: ResourceAddress,
        ) -> Global<FoafGovernance> {
            let (address_reservation, _) =
                Runtime::allocate_component_address(FoafGovernance::blueprint_id());
            Self {
                staking_component,
                rfoaf_resource,
                proposals: KeyValueStore::new(),
                proposal_count: 0,
                quorum_percentage: dec!("0.10"),
                voting_duration_epochs: 2016,
                admin_badge,
                council_badge,
                proposal_threshold: dec!("50000"),
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

        /// Submit a new proposal. Caller must hold >= proposal_threshold rFOAF.
        pub fn submit_proposal(
            &mut self,
            title: String,
            description: String,
            category: ProposalCategory,
            rfoaf_proof: Proof,
        ) -> u64 {
            let checked = rfoaf_proof.check(self.rfoaf_resource);
            assert!(
                checked.amount() >= self.proposal_threshold,
                "Must hold at least {} rFOAF to submit a proposal",
                self.proposal_threshold
            );
            let caller: ComponentAddress = Runtime::global_address().into();
            let current_epoch = Runtime::current_epoch().number();
            let id = self.proposal_count;
            self.proposal_count += 1;
            self.proposals.insert(id, Proposal {
                id,
                title,
                description,
                category,
                proposer: caller,
                created_epoch: current_epoch,
                voting_end_epoch: current_epoch + self.voting_duration_epochs,
                votes_for: dec!("0"),
                votes_against: dec!("0"),
                status: ProposalStatus::Active,
                voters: Vec::new(),
            });
            Runtime::emit_event(ProposalCreatedEvent { id, epoch: current_epoch });
            id
        }

        /// Cast a vote. vote_for=true means in favour.
        pub fn vote(&mut self, proposal_id: u64, vote_for: bool, rfoaf_proof: Proof) {
            let voting_power = rfoaf_proof.check(self.rfoaf_resource).amount();
            let caller: ComponentAddress = Runtime::global_address().into();
            let current_epoch = Runtime::current_epoch().number();
            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");
            assert!(proposal.status == ProposalStatus::Active, "Proposal is not active");
            assert!(current_epoch <= proposal.voting_end_epoch, "Voting period has ended");
            assert!(!proposal.voters.contains(&caller), "Already voted on this proposal");
            if vote_for { proposal.votes_for += voting_power; }
            else { proposal.votes_against += voting_power; }
            proposal.voters.push(caller);
            Runtime::emit_event(VoteCastEvent {
                proposal_id, voter: caller, vote_for, weight: voting_power,
            });
        }

        /// Finalise a proposal after voting ends.
        pub fn execute_proposal(&mut self, proposal_id: u64) {
            let current_epoch = Runtime::current_epoch().number();
            let mut proposal = self.proposals.get_mut(&proposal_id)
                .expect("Proposal not found");
            assert!(proposal.status == ProposalStatus::Active, "Proposal already processed");
            assert!(current_epoch > proposal.voting_end_epoch, "Voting period has not ended");
            let total = proposal.votes_for + proposal.votes_against;
            let passed = total > dec!("0") && match proposal.category {
                ProposalCategory::Standard   => proposal.votes_for > proposal.votes_against,
                ProposalCategory::HighStakes => proposal.votes_for / total >= dec!("0.66"),
                ProposalCategory::Emergency  => proposal.votes_for / total >= dec!("0.51"),
            };
            proposal.status = if passed { ProposalStatus::Passed } else { ProposalStatus::Failed };
            Runtime::emit_event(ProposalExecutedEvent {
                proposal_id, passed,
                votes_for: proposal.votes_for,
                votes_against: proposal.votes_against,
            });
        }

        pub fn get_proposal(&self, id: u64) -> Proposal {
            self.proposals.get(&id).map(|p| p.clone()).expect("Proposal does not exist")
        }

        pub fn list_proposals(&self, from: u64, count: u64) -> Vec<Proposal> {
            (from..from + count)
                .filter_map(|i| self.proposals.get(&i).map(|p| p.clone()))
                .collect()
        }

        pub fn get_voting_power(&self, rfoaf_proof: Proof) -> Decimal {
            rfoaf_proof.check(self.rfoaf_resource).amount()
        }
    }
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ProposalCreatedEvent { pub id: u64, pub epoch: u64 }

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct VoteCastEvent {
    pub proposal_id: u64, pub voter: ComponentAddress,
    pub vote_for: bool, pub weight: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct ProposalExecutedEvent {
    pub proposal_id: u64, pub passed: bool,
    pub votes_for: Decimal, pub votes_against: Decimal,
}
