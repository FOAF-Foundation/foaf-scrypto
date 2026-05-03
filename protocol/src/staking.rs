use scrypto::prelude::*;

#[derive(ScryptoSbor, Clone, Debug)]
pub struct StakePosition {
    pub foaf_amount: Decimal,
    pub stake_epoch: u64,
    pub lock_duration_epochs: u64,
    pub multiplier: Decimal,
}

/// Soulbound voter identity badge data
#[derive(ScryptoSbor, NonFungibleData)]
pub struct VoterBadgeData {
    /// Account address this badge is bound to
    pub account: ComponentAddress,
    /// Epoch when badge was issued
    pub issued_epoch: u64,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct RheoConsumedEvent {
    pub account: ComponentAddress,
    pub amount: Decimal,
    pub epoch: u64,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct VoterBadgeIssuedEvent {
    pub account: ComponentAddress,
    pub local_id: NonFungibleLocalId,
    pub epoch: u64,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct TierHolderCountChangedEvent {
    pub tier: u8,
    pub new_count: u64,
}

#[blueprint]
#[events(RheoConsumedEvent, VoterBadgeIssuedEvent, TierHolderCountChangedEvent)]
mod staking_module {
    enable_method_auth! {
        roles {
            admin => updatable_by: [OWNER];
            protocol => updatable_by: [OWNER];
        },
        methods {
            stake_vfoaf          => PUBLIC;
            stake_rfoaf          => PUBLIC;
            unstake_vfoaf        => PUBLIC;
            unstake_rfoaf        => PUBLIC;
            get_stake_position   => PUBLIC;
            get_accrued_rheo     => PUBLIC;
            get_voter_badge_id   => PUBLIC;
            qualifying_count     => PUBLIC;
            voter_badge_resource => PUBLIC;
            consume_rheo         => restrict_to: [protocol, admin];
            update_foaf_address  => restrict_to: [admin];
        }
    }

    struct FoafStaking {
        foaf_resource: ResourceAddress,
        foaf_vault: Vault,
        vfoaf_resource: ResourceAddress,
        rfoaf_resource: ResourceAddress,
        vfoaf_manager: FungibleResourceManager,
        rfoaf_manager: FungibleResourceManager,

        /// Soulbound voter identity badge manager
        voter_badge_manager: NonFungibleResourceManager,
        voter_badge_resource: ResourceAddress,

        /// Track which accounts have been issued a voter badge
        /// account -> NonFungibleLocalId
        voter_badge_ids: KeyValueStore<ComponentAddress, NonFungibleLocalId>,

        /// Monotonic counter for voter badge local_ids
        next_voter_id: u64,

        /// Count of accounts qualifying per tier (0-indexed: 0=Tier1, 1=Tier2, 2=Tier3, 3=Tier4)
        /// Updated when rFOAF balance crosses a tier threshold
        tier_holder_counts: KeyValueStore<u8, u64>,

        /// Track rFOAF balance per account for threshold crossing detection
        rfoaf_balances: KeyValueStore<ComponentAddress, Decimal>,

        stake_positions: KeyValueStore<ComponentAddress, Vec<StakePosition>>,
        rheo_base_rate: Decimal,
        admin_badge: ResourceAddress,
        protocol_badge: ResourceAddress,
    }

    impl FoafStaking {
        pub fn instantiate(
            foaf_resource: ResourceAddress,
            rheo_base_rate: Decimal,
        ) -> (Global<FoafStaking>, Bucket, Bucket) {
            let (address_reservation, component_address) =
                Runtime::allocate_component_address(FoafStaking::blueprint_id());

            let admin_badge: FungibleBucket = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata! {
                    init {
                        "name" => "FOAF Admin Badge", locked;
                        "symbol" => "FOAF-ADMIN", locked;
                    }
                })
                .mint_initial_supply(1);

            let protocol_badge: FungibleBucket = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata! {
                    init {
                        "name" => "FOAF Protocol Badge", locked;
                        "symbol" => "FOAF-PROTO", locked;
                    }
                })
                .mint_initial_supply(1);

            // vFOAF — soulbound, non-transferable
            let vfoaf_manager: FungibleResourceManager = ResourceBuilder::new_fungible(
                OwnerRole::Fixed(rule!(require(global_caller(component_address))))
            )
            .divisibility(DIVISIBILITY_NONE)
            .metadata(metadata! {
                init {
                    "name" => "Vote-Escrowed FOAF", locked;
                    "symbol" => "vFOAF", locked;
                    "description" => "Soulbound staking receipt. Non-transferable.", locked;
                }
            })
            .deposit_roles(deposit_roles! {
                depositor => rule!(allow_all);
                depositor_updater => rule!(deny_all);
            })
            .withdraw_roles(withdraw_roles! {
                withdrawer => rule!(deny_all);
                withdrawer_updater => rule!(deny_all);
            })
            .burn_roles(burn_roles! {
                burner => rule!(require(global_caller(component_address)));
                burner_updater => rule!(deny_all);
            })
            .mint_roles(mint_roles! {
                minter => rule!(require(global_caller(component_address)));
                minter_updater => rule!(deny_all);
            })
            .create_with_no_initial_supply();

            // rFOAF — soulbound, non-transferable, time-locked
            let rfoaf_manager: FungibleResourceManager = ResourceBuilder::new_fungible(
                OwnerRole::Fixed(rule!(require(global_caller(component_address))))
            )
            .divisibility(DIVISIBILITY_NONE)
            .metadata(metadata! {
                init {
                    "name" => "Rooted FOAF", locked;
                    "symbol" => "rFOAF", locked;
                    "description" => "Time-locked soulbound staking receipt. Non-transferable.", locked;
                }
            })
            .deposit_roles(deposit_roles! {
                depositor => rule!(allow_all);
                depositor_updater => rule!(deny_all);
            })
            .withdraw_roles(withdraw_roles! {
                withdrawer => rule!(deny_all);
                withdrawer_updater => rule!(deny_all);
            })
            .burn_roles(burn_roles! {
                burner => rule!(require(global_caller(component_address)));
                burner_updater => rule!(deny_all);
            })
            .mint_roles(mint_roles! {
                minter => rule!(require(global_caller(component_address)));
                minter_updater => rule!(deny_all);
            })
            .create_with_no_initial_supply();

            // Voter identity badge — soulbound NonFungible, permanent
            // Issued once per account when stake crosses Tier 1 (10 rFOAF)
            // local_id is the canonical voter identity
            let voter_badge_manager: NonFungibleResourceManager = ResourceBuilder::new_integer_non_fungible::<VoterBadgeData>(
                OwnerRole::Fixed(rule!(require(global_caller(component_address))))
            )
            .metadata(metadata! {
                init {
                    "name" => "FOAF Voter Identity Badge", locked;
                    "symbol" => "FOAF-VOTER", locked;
                    "description" => "Soulbound voter identity. Permanent. One per account.", locked;
                }
            })
            .deposit_roles(deposit_roles! {
                depositor => rule!(allow_all);
                depositor_updater => rule!(deny_all);
            })
            .withdraw_roles(withdraw_roles! {
                withdrawer => rule!(deny_all);
                withdrawer_updater => rule!(deny_all);
            })
            .burn_roles(burn_roles! {
                burner => rule!(deny_all);
                burner_updater => rule!(deny_all);
            })
            .mint_roles(mint_roles! {
                minter => rule!(require(global_caller(component_address)));
                minter_updater => rule!(deny_all);
            })
            .create_with_no_initial_supply();

            let vfoaf_resource = vfoaf_manager.address();
            let rfoaf_resource = rfoaf_manager.address();
            let voter_badge_resource = voter_badge_manager.address();
            let admin_badge_addr = admin_badge.resource_address();
            let protocol_badge_addr = protocol_badge.resource_address();

            let component = Self {
                foaf_resource,
                foaf_vault: Vault::new(foaf_resource),
                vfoaf_resource,
                rfoaf_resource,
                vfoaf_manager,
                rfoaf_manager,
                voter_badge_manager,
                voter_badge_resource,
                voter_badge_ids: KeyValueStore::new(),
                next_voter_id: 0,
                tier_holder_counts: KeyValueStore::new(),
                rfoaf_balances: KeyValueStore::new(),
                stake_positions: KeyValueStore::new(),
                rheo_base_rate,
                admin_badge: admin_badge_addr,
                protocol_badge: protocol_badge_addr,
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(admin_badge_addr))))
            .with_address(address_reservation)
            .roles(roles! {
                admin => rule!(require(admin_badge_addr));
                protocol => rule!(require(protocol_badge_addr));
            })
            .globalize();

            (component, admin_badge.into(), protocol_badge.into())
        }

        /// Stake FOAF -> receive vFOAF (flexible, 1:1)
        /// Issues voter badge if account crosses Tier 1 for the first time
        pub fn stake_vfoaf(&mut self, foaf_bucket: Bucket, caller: ComponentAddress) -> Bucket {
            assert!(
                foaf_bucket.resource_address() == self.foaf_resource,
                "Wrong token: must be FOAF"
            );
            assert!(foaf_bucket.amount() >= dec!("1"), "Minimum stake is 1 FOAF");

            let amount = foaf_bucket.amount();
            let current_epoch = Runtime::current_epoch().number();

            let mut positions = self.stake_positions
                .get_mut(&caller).map(|p| p.clone()).unwrap_or_default();
            positions.push(StakePosition {
                foaf_amount: amount,
                stake_epoch: current_epoch,
                lock_duration_epochs: 0,
                multiplier: dec!("1"),
            });
            self.stake_positions.insert(caller, positions);
            self.foaf_vault.put(foaf_bucket);
            self.vfoaf_manager.mint(amount).into()
        }

        /// Stake FOAF -> receive rFOAF (time-locked, multiplier 1.0x-4.0x)
        /// Updates tier_holder_counts and issues voter badge if needed
        pub fn stake_rfoaf(
            &mut self,
            foaf_bucket: Bucket,
            lock_duration_epochs: u64,
            caller: ComponentAddress,
        ) -> (Bucket, Option<Bucket>) {
            assert!(
                foaf_bucket.resource_address() == self.foaf_resource,
                "Wrong token: must be FOAF"
            );
            assert!(
                foaf_bucket.amount() >= dec!("100"),
                "Minimum stake for rFOAF is 100 FOAF"
            );

            let min_lock: u64 = 52560;
            let max_lock: u64 = 210240;
            assert!(lock_duration_epochs >= min_lock, "Minimum lock: 6 months (52560 epochs)");
            assert!(lock_duration_epochs <= max_lock, "Maximum lock: 2 years (210240 epochs)");

            let amount = foaf_bucket.amount();
            let current_epoch = Runtime::current_epoch().number();

            let multiplier = dec!("1")
                + dec!("3") * Decimal::from(lock_duration_epochs) / Decimal::from(max_lock);

            let mut positions = self.stake_positions
                .get_mut(&caller).map(|p| p.clone()).unwrap_or_default();
            positions.push(StakePosition {
                foaf_amount: amount,
                stake_epoch: current_epoch,
                lock_duration_epochs,
                multiplier,
            });
            self.stake_positions.insert(caller, positions);
            self.foaf_vault.put(foaf_bucket);

            // Track old vs new rFOAF balance for tier crossing detection
            let old_balance = self.rfoaf_balances.get(&caller)
                .map(|b| *b).unwrap_or(dec!("0"));
            let new_balance = old_balance + amount;
            self.rfoaf_balances.insert(caller, new_balance);

            // Update tier holder counts for crossed thresholds
            self.update_tier_counts(caller, old_balance, new_balance);

            // Issue voter badge if first time crossing Tier 1 (10 rFOAF)
            let voter_badge_bucket = self.maybe_issue_voter_badge(caller, old_balance, new_balance, current_epoch);

            let rfoaf_bucket: Bucket = self.rfoaf_manager.mint(amount).into();
            (rfoaf_bucket, voter_badge_bucket)
        }

        /// Unstake vFOAF -> return FOAF
        pub fn unstake_vfoaf(&mut self, vfoaf_bucket: Bucket, caller: ComponentAddress) -> Bucket {
            assert!(
                vfoaf_bucket.resource_address() == self.vfoaf_resource,
                "Must provide vFOAF"
            );
            let amount = vfoaf_bucket.amount();
            self.remove_stake_position(caller, amount, 0);
            self.vfoaf_manager.burn(vfoaf_bucket);
            self.foaf_vault.take(amount)
        }

        /// Unstake rFOAF -> return FOAF (only after lock expired)
        /// Updates tier_holder_counts. Voter badge is NOT burned (permanent identity).
        pub fn unstake_rfoaf(
            &mut self,
            rfoaf_bucket: Bucket,
            caller: ComponentAddress,
            position_index: usize,
        ) -> Bucket {
            assert!(
                rfoaf_bucket.resource_address() == self.rfoaf_resource,
                "Must provide rFOAF"
            );
            let current_epoch = Runtime::current_epoch().number();

            let positions = self.stake_positions.get(&caller)
                .expect("No stake position found");
            let position = positions.get(position_index)
                .expect("Invalid position index");

            let unlock_epoch = position.stake_epoch + position.lock_duration_epochs;
            assert!(
                current_epoch >= unlock_epoch,
                "rFOAF still locked until epoch {}",
                unlock_epoch
            );
            let foaf_amount = position.foaf_amount;
            drop(positions);

            let old_balance = self.rfoaf_balances.get(&caller)
                .map(|b| *b).unwrap_or(dec!("0"));
            let new_balance = if old_balance >= foaf_amount {
                old_balance - foaf_amount
            } else {
                dec!("0")
            };
            self.rfoaf_balances.insert(caller, new_balance);

            // Update tier counts for thresholds crossed downward
            self.update_tier_counts(caller, old_balance, new_balance);

            self.remove_stake_position_by_index(caller, position_index);
            self.rfoaf_manager.burn(rfoaf_bucket);
            self.foaf_vault.take(foaf_amount)
        }

        /// View: all stake positions for an account
        pub fn get_stake_position(&self, account: ComponentAddress) -> Vec<StakePosition> {
            self.stake_positions.get(&account).map(|p| p.clone()).unwrap_or_default()
        }

        /// View: compute accrued RHEO (lazy — not stored, computed on demand)
        pub fn get_accrued_rheo(&self, account: ComponentAddress) -> Decimal {
            let current_epoch = Runtime::current_epoch().number();
            let positions = self.stake_positions.get(&account)
                .map(|p| p.clone()).unwrap_or_default();
            positions.iter().fold(dec!("0"), |acc, pos| {
                let elapsed = Decimal::from(current_epoch - pos.stake_epoch);
                acc + pos.foaf_amount * elapsed * self.rheo_base_rate * pos.multiplier
            })
        }

        /// View: get voter badge NonFungibleLocalId for an account (if issued)
        pub fn get_voter_badge_id(&self, account: ComponentAddress) -> Option<NonFungibleLocalId> {
            self.voter_badge_ids.get(&account).map(|id| id.clone())
        }

        /// View: number of accounts qualifying for a given tier
        /// tier: 1=Tier1, 2=Tier2, 3=Tier3, 4=Tier4
        pub fn voter_badge_resource(&self) -> ResourceAddress {
            self.voter_badge_resource
        }

        pub fn qualifying_count(&self, tier: u8) -> u64 {
            self.tier_holder_counts.get(&tier).map(|c| *c).unwrap_or(0)
        }

        /// Consume RHEO within same manifest (lazy accrual + burn pattern)
        pub fn consume_rheo(&mut self, account: ComponentAddress, amount_needed: Decimal) {
            let available = self.get_accrued_rheo(account);
            assert!(
                available >= amount_needed,
                "Insufficient RHEO. Available: {}, Required: {}",
                available, amount_needed
            );
            self.deduct_rheo_accrual(account, amount_needed);
            Runtime::emit_event(RheoConsumedEvent {
                account,
                amount: amount_needed,
                epoch: Runtime::current_epoch().number(),
            });
        }

        pub fn update_foaf_address(&mut self, new_addr: ResourceAddress) {
            self.foaf_resource = new_addr;
        }

        // ===== INTERNAL =====

        /// Issue voter badge the first time an account crosses Tier 1 (10 rFOAF)
        fn maybe_issue_voter_badge(
            &mut self,
            account: ComponentAddress,
            old_balance: Decimal,
            new_balance: Decimal,
            current_epoch: u64,
        ) -> Option<Bucket> {
            let tier1_threshold = dec!("10");
            let already_has_badge = self.voter_badge_ids.get(&account).is_some();

            if !already_has_badge && old_balance < tier1_threshold && new_balance >= tier1_threshold {
                let local_id = NonFungibleLocalId::integer(self.next_voter_id);
                self.next_voter_id += 1;

                let badge_bucket = self.voter_badge_manager.mint_non_fungible(
                    &local_id,
                    VoterBadgeData { account, issued_epoch: current_epoch },
                );

                self.voter_badge_ids.insert(account, local_id.clone());

                Runtime::emit_event(VoterBadgeIssuedEvent {
                    account,
                    local_id,
                    epoch: current_epoch,
                });

                Some(badge_bucket.into())
            } else {
                None
            }
        }

        /// Update tier_holder_counts when an account's rFOAF balance changes
        fn update_tier_counts(
            &mut self,
            _account: ComponentAddress,
            old_balance: Decimal,
            new_balance: Decimal,
        ) {
            let thresholds: [(u8, Decimal); 4] = [
                (1, dec!("10")),
                (2, dec!("100")),
                (3, dec!("1000")),
                (4, dec!("10000")),
            ];

            for (tier, threshold) in thresholds.iter() {
                let was_qualified = old_balance >= *threshold;
                let is_qualified = new_balance >= *threshold;

                if !was_qualified && is_qualified {
                    // Crossed threshold upward — increment
                    let count = self.tier_holder_counts.get(tier).map(|c| *c).unwrap_or(0);
                    let new_count = count + 1;
                    self.tier_holder_counts.insert(*tier, new_count);
                    Runtime::emit_event(TierHolderCountChangedEvent { tier: *tier, new_count });
                } else if was_qualified && !is_qualified {
                    // Crossed threshold downward — decrement
                    let count = self.tier_holder_counts.get(tier).map(|c| *c).unwrap_or(0);
                    let new_count = if count > 0 { count - 1 } else { 0 };
                    self.tier_holder_counts.insert(*tier, new_count);
                    Runtime::emit_event(TierHolderCountChangedEvent { tier: *tier, new_count });
                }
            }
        }

        fn remove_stake_position(
            &mut self,
            account: ComponentAddress,
            amount: Decimal,
            lock_duration: u64,
        ) {
            let mut positions = self.stake_positions.get(&account)
                .map(|p| p.clone()).unwrap_or_default();
            let idx = positions.iter().position(|p| {
                p.foaf_amount == amount && p.lock_duration_epochs == lock_duration
            }).expect("Stake position not found");
            positions.remove(idx);
            self.stake_positions.insert(account, positions);
        }

        fn remove_stake_position_by_index(&mut self, account: ComponentAddress, index: usize) {
            let mut positions = self.stake_positions.get(&account)
                .map(|p| p.clone()).unwrap_or_default();
            positions.remove(index);
            self.stake_positions.insert(account, positions);
        }

        fn deduct_rheo_accrual(&mut self, account: ComponentAddress, amount_consumed: Decimal) {
            let current_epoch = Runtime::current_epoch().number();
            let mut positions = self.stake_positions.get(&account)
                .map(|p| p.clone()).unwrap_or_default();
            let mut remaining = amount_consumed;
            for pos in positions.iter_mut() {
                if remaining <= dec!("0") { break; }
                let elapsed = Decimal::from(current_epoch - pos.stake_epoch);
                let pos_rheo = pos.foaf_amount * elapsed * self.rheo_base_rate * pos.multiplier;
                if pos_rheo <= remaining {
                    remaining -= pos_rheo;
                    pos.stake_epoch = current_epoch;
                } else {
                    let epochs_to_deduct = remaining
                        / (pos.foaf_amount * self.rheo_base_rate * pos.multiplier);
                    let epochs_u64 = epochs_to_deduct.to_string().parse::<u64>().unwrap_or(0);
                    pos.stake_epoch += epochs_u64;
                    remaining = dec!("0");
                }
            }
            self.stake_positions.insert(account, positions);
        }
    }
}
