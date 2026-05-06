use scrypto::prelude::*;

#[derive(ScryptoSbor, Clone, Debug)]
pub struct StakePosition {
    pub position_id: u64,
    pub foaf_amount: Decimal,
    pub stake_epoch: u64,
    pub lock_duration_epochs: u64,
    pub multiplier: Decimal,
}

#[derive(ScryptoSbor, NonFungibleData)]
pub struct VoterBadgeData {
    pub account: ComponentAddress,
    pub issued_epoch: u64,
}

/// Soulbound NFT receipt issued per vFOAF stake position
/// Used for unstake identity proof — decoupled from rFOAF tier system
#[derive(ScryptoSbor, NonFungibleData)]
pub struct VStakeReceiptData {
    pub account: ComponentAddress,
    pub position_id: u64,
    pub foaf_amount: Decimal,
    pub stake_epoch: u64,
    pub issued_epoch: u64,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct RheoConsumedEvent {
    pub account: ComponentAddress,
    pub amount: Decimal,
    pub epoch: u64,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct VStakeReceiptIssuedEvent {
    pub account: ComponentAddress,
    pub local_id: NonFungibleLocalId,
    pub foaf_amount: Decimal,
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
#[events(RheoConsumedEvent, VStakeReceiptIssuedEvent, VoterBadgeIssuedEvent, TierHolderCountChangedEvent)]
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
            vfoaf_receipt_resource => PUBLIC;
            get_rfoaf_balance_for_voter => PUBLIC;
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
        /// vFOAF stake receipt manager — one NFT per stake position
        vfoaf_receipt_manager: NonFungibleResourceManager,
        vfoaf_receipt_resource: ResourceAddress,
        next_vstake_id: u64,
        next_position_id: u64,
        /// Track burned/used stake receipts to prevent replay
        used_vstake_receipts: KeyValueStore<NonFungibleLocalId, bool>,

        voter_badge_manager: NonFungibleResourceManager,
        voter_badge_resource: ResourceAddress,
        voter_badge_ids: KeyValueStore<ComponentAddress, NonFungibleLocalId>,
        next_voter_id: u64,
        tier_holder_counts: KeyValueStore<u8, u64>,
        rfoaf_balances: KeyValueStore<ComponentAddress, Decimal>,
        /// Internal vault holding all vFOAF (not sent to user accounts)
        vfoaf_vault: Vault,
        /// Internal vault holding all rFOAF receipts (not sent to user accounts)
        rfoaf_vault: Vault,
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

            // vFOAF — soulbound, deposit allowed, withdraw denied
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

            // rFOAF — stored in component vault per account (not in user account)
            // This is intentional: soulbound receipt lives in staking component
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

            // vFOAF stake receipt — soulbound NFT, one per stake position
            // Used for unstake identity proof, decoupled from rFOAF tier system
            let vfoaf_receipt_manager: NonFungibleResourceManager =
                ResourceBuilder::new_integer_non_fungible::<VStakeReceiptData>(
                    OwnerRole::Fixed(rule!(require(global_caller(component_address))))
                )
                .metadata(metadata! {
                    init {
                        "name" => "FOAF vFOAF Stake Receipt", locked;
                        "symbol" => "FOAF-VSTAKE", locked;
                        "description" => "Soulbound stake receipt per vFOAF position. Used for unstake proof.", locked;
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
            let voter_badge_manager: NonFungibleResourceManager =
                ResourceBuilder::new_integer_non_fungible::<VoterBadgeData>(
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
            let vfoaf_receipt_resource = vfoaf_receipt_manager.address();
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
                vfoaf_receipt_manager,
                vfoaf_receipt_resource,
                next_vstake_id: 0,
                next_position_id: 0,
                used_vstake_receipts: KeyValueStore::new(),
                vfoaf_vault: Vault::new(vfoaf_resource),
                rfoaf_vault: Vault::new(rfoaf_resource),
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
        /// Stake FOAF -> receive vFOAF stake receipt NFT (soulbound, per-position)
        /// vFOAF stored internally in component vault
        /// Returns: vFOAF stake receipt bucket
        pub fn stake_vfoaf(&mut self, foaf_bucket: Bucket, account: ComponentAddress) -> Bucket {
            assert!(
                foaf_bucket.resource_address() == self.foaf_resource,
                "Wrong token: must be FOAF"
            );
            assert!(foaf_bucket.amount() >= dec!("1"), "Minimum stake is 1 FOAF");

            let caller = account;
            let amount = foaf_bucket.amount();
            let current_epoch = Runtime::current_epoch().number();

            let position_id = self.next_position_id;
            self.next_position_id += 1;

            let mut positions = self.stake_positions
                .get_mut(&caller).map(|p| p.clone()).unwrap_or_default();
            positions.push(StakePosition {
                position_id,
                foaf_amount: amount,
                stake_epoch: current_epoch,
                lock_duration_epochs: 0,
                multiplier: dec!("1"),
            });
            self.stake_positions.insert(caller, positions);
            self.foaf_vault.put(foaf_bucket);

            // Mint vFOAF internally
            let vfoaf_bucket: Bucket = self.vfoaf_manager.mint(amount).into();
            self.vfoaf_vault.put(vfoaf_bucket);

            // Issue soulbound stake receipt NFT — 1:1 with position via position_id
            let receipt_id = NonFungibleLocalId::integer(self.next_vstake_id);
            self.next_vstake_id += 1;
            let receipt = self.vfoaf_receipt_manager.mint_non_fungible(
                &receipt_id,
                VStakeReceiptData {
                    account: caller,
                    position_id,
                    foaf_amount: amount,
                    stake_epoch: current_epoch,
                    issued_epoch: current_epoch,
                },
            );
            Runtime::emit_event(VStakeReceiptIssuedEvent {
                account: caller,
                local_id: receipt_id,
                foaf_amount: amount,
                epoch: current_epoch,
            });
            receipt.into()
        }

        /// Stake FOAF -> receive voter badge if first time crossing Tier 1
        /// rFOAF is stored internally in component vault (soulbound receipt)
        pub fn stake_rfoaf(
            &mut self,
            foaf_bucket: Bucket,
            lock_duration_epochs: u64,
            account: ComponentAddress,
        ) -> Option<Bucket> {
            assert!(
                foaf_bucket.resource_address() == self.foaf_resource,
                "Wrong token: must be FOAF"
            );
            assert!(foaf_bucket.amount() >= dec!("100"), "Minimum stake for rFOAF is 100 FOAF");

            let min_lock: u64 = 52560;
            let max_lock: u64 = 210240;
            assert!(lock_duration_epochs >= min_lock, "Minimum lock: 6 months (52560 epochs)");
            assert!(lock_duration_epochs <= max_lock, "Maximum lock: 2 years (210240 epochs)");

            let caller = account;
            let amount = foaf_bucket.amount();
            let current_epoch = Runtime::current_epoch().number();
            let multiplier = dec!("1")
                + dec!("3") * Decimal::from(lock_duration_epochs) / Decimal::from(max_lock);

            let position_id = self.next_position_id;
            self.next_position_id += 1;

            let mut positions = self.stake_positions
                .get_mut(&caller).map(|p| p.clone()).unwrap_or_default();
            positions.push(StakePosition {
                position_id,
                foaf_amount: amount,
                stake_epoch: current_epoch,
                lock_duration_epochs,
                multiplier,
            });
            self.stake_positions.insert(caller, positions);
            self.foaf_vault.put(foaf_bucket);

            let old_balance = self.rfoaf_balances.get(&caller)
                .map(|b| *b).unwrap_or(dec!("0"));
            let new_balance = old_balance + amount;
            self.rfoaf_balances.insert(caller, new_balance);
            self.update_tier_counts(caller, old_balance, new_balance);

            let voter_badge_bucket = self.maybe_issue_voter_badge(
                caller, old_balance, new_balance, current_epoch
            );

            // Store rFOAF in internal component vault (soulbound — not transferable)
            let rfoaf_bucket: Bucket = self.rfoaf_manager.mint(amount).into();
            self.rfoaf_vault.put(rfoaf_bucket);
            voter_badge_bucket
        }

        /// Unstake vFOAF -> return FOAF
        /// Requires stake receipt NFT proof — identity bound to soulbound receipt
        pub fn unstake_vfoaf(&mut self, receipt_proof: Proof) -> Bucket {
            let receipt_checked = receipt_proof.check(self.vfoaf_receipt_resource);
            let receipt_ids = receipt_checked.as_non_fungible().non_fungible_local_ids();
            assert!(receipt_ids.len() == 1, "Must provide exactly one stake receipt");
            let receipt_id = receipt_ids.into_iter().next().unwrap();

            // Prevent replay: check receipt not already used
            assert!(
                self.used_vstake_receipts.get(&receipt_id).is_none(),
                "Stake receipt already used"
            );
            // Mark receipt as used immediately
            self.used_vstake_receipts.insert(receipt_id.clone(), true);

            // Get position data from receipt NFT
            let receipt_data: VStakeReceiptData = self.vfoaf_receipt_manager
                .get_non_fungible_data(&receipt_id);
            let caller = receipt_data.account;
            let amount = receipt_data.foaf_amount;

            // Remove by exact position_id — receipt maps 1:1 to position
            self.remove_stake_position_by_id(caller, receipt_data.position_id);

            // Burn vFOAF from internal vault
            let vfoaf_bucket = self.vfoaf_vault.take(amount);
            self.vfoaf_manager.burn(vfoaf_bucket);

            // Drop proof — receipt stays in user account, replay blocked by used_vstake_receipts
            drop(receipt_checked);

            self.foaf_vault.take(amount)
        }

        /// Unstake rFOAF -> return FOAF (only after lock expired)
        /// Takes amount + caller instead of bucket since rFOAF is soulbound
        /// Component withdraws rFOAF from caller account internally
        pub fn unstake_rfoaf(
            &mut self,
            amount: Decimal,
            position_index: usize,
            voter_badge_proof: Proof,
        ) -> Bucket {
            // SECURITY: caller identity bound to voter badge — unforgeable
            // Eve cannot unstake Alice's position because she cannot produce
            // a proof of Alice's soulbound voter badge
            let vbr = self.voter_badge_resource;
            let badge_checked = voter_badge_proof.check(vbr);
            let voter_ids = badge_checked.as_non_fungible().non_fungible_local_ids();
            assert!(voter_ids.len() == 1, "Must provide exactly one voter badge");
            let voter_id = voter_ids.into_iter().next().unwrap();

            // Get account address from badge data
            let badge_data: VoterBadgeData = self.voter_badge_manager
                .get_non_fungible_data(&voter_id);
            let caller = badge_data.account;
            let current_epoch = Runtime::current_epoch().number();

            let positions = self.stake_positions.get(&caller)
                .expect("No stake position found");
            let position = positions.get(position_index)
                .expect("Invalid position index");

            assert!(
                position.foaf_amount == amount,
                "Amount does not match position"
            );

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
            } else { dec!("0") };
            self.rfoaf_balances.insert(caller, new_balance);
            self.update_tier_counts(caller, old_balance, new_balance);
            self.remove_stake_position_by_index(caller, position_index);

            // Burn rFOAF from internal component vault
            let rfoaf_bucket = self.rfoaf_vault.take(foaf_amount);
            self.rfoaf_manager.burn(rfoaf_bucket);
            self.foaf_vault.take(foaf_amount)
        }

        pub fn get_stake_position(&self, account: ComponentAddress) -> Vec<StakePosition> {
            self.stake_positions.get(&account).map(|p| p.clone()).unwrap_or_default()
        }

        pub fn get_accrued_rheo(&self, account: ComponentAddress) -> Decimal {
            let current_epoch = Runtime::current_epoch().number();
            let positions = self.stake_positions.get(&account)
                .map(|p| p.clone()).unwrap_or_default();
            positions.iter().fold(dec!("0"), |acc, pos| {
                let elapsed = Decimal::from(current_epoch - pos.stake_epoch);
                acc + pos.foaf_amount * elapsed * self.rheo_base_rate * pos.multiplier
            })
        }

        pub fn get_voter_badge_id(&self, account: ComponentAddress) -> Option<NonFungibleLocalId> {
            self.voter_badge_ids.get(&account).map(|id| id.clone())
        }

        pub fn voter_badge_resource(&self) -> ResourceAddress {
            self.voter_badge_resource
        }

        pub fn vfoaf_receipt_resource(&self) -> ResourceAddress {
            self.vfoaf_receipt_resource
        }

        /// Look up rFOAF balance for a voter by their badge local_id
        /// Used by governance to check tier qualification without requiring rFOAF proof
        /// (rFOAF lives in internal vault — user cannot produce a proof of it)
        pub fn get_rfoaf_balance_for_voter(&self, voter_id: NonFungibleLocalId) -> Decimal {
            let data: VoterBadgeData = self.voter_badge_manager
                .get_non_fungible_data(&voter_id);
            self.rfoaf_balances.get(&data.account)
                .map(|b| *b)
                .unwrap_or(dec!("0"))
        }

        pub fn qualifying_count(&self, tier: u8) -> u64 {
            self.tier_holder_counts.get(&tier).map(|c| *c).unwrap_or(0)
        }

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
                    account, local_id, epoch: current_epoch,
                });
                Some(badge_bucket.into())
            } else {
                None
            }
        }

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
                    let count = self.tier_holder_counts.get(tier).map(|c| *c).unwrap_or(0);
                    let new_count = count + 1;
                    self.tier_holder_counts.insert(*tier, new_count);
                    Runtime::emit_event(TierHolderCountChangedEvent { tier: *tier, new_count });
                } else if was_qualified && !is_qualified {
                    let count = self.tier_holder_counts.get(tier).map(|c| *c).unwrap_or(0);
                    let new_count = if count > 0 { count - 1 } else { 0 };
                    self.tier_holder_counts.insert(*tier, new_count);
                    Runtime::emit_event(TierHolderCountChangedEvent { tier: *tier, new_count });
                }
            }
        }

        fn remove_stake_position_by_id(
            &mut self,
            account: ComponentAddress,
            position_id: u64,
        ) {
            let mut positions = self.stake_positions.get(&account)
                .map(|p| p.clone()).unwrap_or_default();
            let idx = positions.iter().position(|p| p.position_id == position_id)
                .expect("Stake position not found for given position_id");
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
                    // checked_floor to avoid silent-zero on non-integer Decimal
                    let epochs_u64 = epochs_to_deduct
                        .checked_floor()
                        .and_then(|d| d.to_string().parse::<u64>().ok())
                        .unwrap_or(0);
                    pos.stake_epoch += epochs_u64;
                    remaining = dec!("0");
                }
            }
            self.stake_positions.insert(account, positions);
        }
    }
}
