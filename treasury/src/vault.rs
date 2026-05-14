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
            deposit          => restrict_to: [admin];
            disburse         => restrict_to: [governance];
            get_balance      => PUBLIC;
            emergency_lock   => restrict_to: [admin];
            emergency_unlock => restrict_to: [governance];
        }
    }

    struct FoafTreasury {
        /// Main vault — holds the Foundation FOAF reserve (3M FOAF, 12% of supply)
        foaf_vault: Vault,
        /// When true, all disbursements are halted. Set by admin (emergency_lock),
        /// cleared only by governance (emergency_unlock) to prevent single-admin abuse.
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

        /// Deposit FOAF into the treasury (admin only).
        pub fn deposit(&mut self, foaf_bucket: Bucket) {
            assert!(!self.locked, "Treasury is in emergency lock mode");
            self.foaf_vault.put(foaf_bucket);
        }

        /// Disburse FOAF following a passed governance proposal.
        /// Deposits atomically into the recipient account; does NOT return a free Bucket
        /// so the caller cannot redirect the funds elsewhere.
        pub fn disburse(&mut self, amount: Decimal, recipient: ComponentAddress) {
            assert!(!self.locked, "Treasury is in emergency lock mode");
            assert!(self.foaf_vault.amount() >= amount, "Insufficient treasury balance");
            assert!(amount > dec!("0"), "Disbursement amount must be positive");

            let bucket = self.foaf_vault.take(amount);
            self.total_disbursed += amount;

            let remaining = self.foaf_vault.amount();
            Runtime::emit_event(TreasuryDisbursementEvent { recipient, amount, remaining });

            // Atomic deposit into the recipient account. If the recipient rejects the
            // deposit (default-deny rule, etc.), the entire transaction aborts and the
            // disburse call has no effect.
            Global::<Account>::from(recipient).try_deposit_or_abort(bucket, None);
        }

        pub fn get_balance(&self) -> Decimal { self.foaf_vault.amount() }

        /// Engage emergency lock — admin can halt disbursements unilaterally.
        /// Designed to be one-way: only governance can unlock (see emergency_unlock).
        pub fn emergency_lock(&mut self) {
            self.locked = true;
            Runtime::emit_event(EmergencyLockEvent { locked: true });
        }

        /// Release emergency lock — gated to governance so a compromised admin
        /// cannot lock-then-drain by toggling. Requires a passed governance proposal
        /// to be the caller via the governance role.
        pub fn emergency_unlock(&mut self) {
            self.locked = false;
            Runtime::emit_event(EmergencyLockEvent { locked: false });
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
