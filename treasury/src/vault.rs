use scrypto::prelude::*;

#[blueprint]
#[events(TreasuryDisbursementEvent, EmergencyLockEvent)]
mod treasury_module {
    enable_method_auth! {
        roles {
            admin => updatable_by: [OWNER];
            governance => updatable_by: [OWNER];
        },
        methods {
            deposit        => restrict_to: [admin];
            disburse       => restrict_to: [governance];
            get_balance    => PUBLIC;
            emergency_lock => restrict_to: [admin];
        }
    }

    struct FoafTreasury {
        /// Main vault — holds the Foundation FOAF reserve (3M FOAF, 12% of supply)
        foaf_vault: Vault,
        /// When true, all disbursements are halted
        locked: bool,
        admin_badge: ResourceAddress,
        governance_badge: ResourceAddress,
        /// Cumulative FOAF disbursed (audit trail)
        total_disbursed: Decimal,
    }

    impl FoafTreasury {
        pub fn instantiate(
            foaf_resource: ResourceAddress,
            admin_badge: ResourceAddress,
            governance_badge: ResourceAddress,
        ) -> Global<FoafTreasury> {
            let (address_reservation, _) =
                Runtime::allocate_component_address(FoafTreasury::blueprint_id());
            Self {
                foaf_vault: Vault::new(foaf_resource),
                locked: false,
                admin_badge,
                governance_badge,
                total_disbursed: dec!("0"),
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(admin_badge))))
            .with_address(address_reservation)
            .roles(roles! {
                admin => rule!(require(admin_badge));
                governance => rule!(require(governance_badge));
            })
            .globalize()
        }

        /// Deposit FOAF into the treasury (admin only)
        pub fn deposit(&mut self, foaf_bucket: Bucket) {
            assert!(!self.locked, "Treasury is in emergency lock mode");
            self.foaf_vault.put(foaf_bucket);
        }

        /// Disburse FOAF following a passed governance proposal.
        /// Execution flows from proposal -> governance component -> this method, atomic on-chain.
        pub fn disburse(&mut self, amount: Decimal, recipient: ComponentAddress) -> Bucket {
            assert!(!self.locked, "Treasury is in emergency lock mode");
            assert!(self.foaf_vault.amount() >= amount, "Insufficient treasury balance");
            self.total_disbursed += amount;
            Runtime::emit_event(TreasuryDisbursementEvent {
                recipient, amount,
                remaining: self.foaf_vault.amount() - amount,
            });
            self.foaf_vault.take(amount)
        }

        pub fn get_balance(&self) -> Decimal { self.foaf_vault.amount() }

        /// Toggle emergency lock — halts all disbursements when true
        pub fn emergency_lock(&mut self, lock: bool) {
            self.locked = lock;
            Runtime::emit_event(EmergencyLockEvent { locked: lock });
        }
    }
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct TreasuryDisbursementEvent {
    pub recipient: ComponentAddress,
    pub amount: Decimal,
    pub remaining: Decimal,
}

#[derive(ScryptoSbor, ScryptoEvent)]
pub struct EmergencyLockEvent { pub locked: bool }
