#[path = "../../pool/src/fees.rs"]
mod fees;
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{
    env, ext_contract, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise,
    PromiseError, PromiseOrValue,
};

const STORAGE: u128 = 20_000_000_000_000_000_000_000;
const LOCAL_STORAGE: u128 = STORAGE / 4;
const REGISTRY_STORAGE: u128 = STORAGE - LOCAL_STORAGE;
const RESUME_BLOCKS: u64 = 20;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum DepositState {
    Pending,
    Retryable,
    Active,
    Refundable,
    RefundInFlight,
    Refunded,
}
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Deposit {
    pub owner: AccountId,
    pub disclosure_root: String,
    pub state: DepositState,
    pub attempt: u64,
    pub last_height: u64,
}
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum PayoutState {
    Pending,
    Retryable,
    Complete,
}
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Payout {
    pub recipient: AccountId,
    pub disclosure_root: String,
    pub state: PayoutState,
}

#[ext_contract(registry)]
#[allow(dead_code)]
trait Registry {
    fn register_deposit(&mut self, commitment: String, disclosure_root: String) -> bool;
}
#[ext_contract(callback)]
#[allow(dead_code)]
trait Callbacks {
    fn on_registered(
        &mut self,
        commitment: String,
        attempt: u64,
        #[callback_result] result: Result<bool, PromiseError>,
    ) -> bool;
    fn on_payout(
        &mut self,
        nullifier: String,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool;
    fn on_refund(
        &mut self,
        commitment: String,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool;
    fn on_fees(
        &mut self,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Vault {
    registry: AccountId,
    network: String,
    denomination: u128,
    fee_recipient: AccountId,
    deposits: LookupMap<String, Deposit>,
    payouts: LookupMap<String, Payout>,
    pending: u32,
    available: u32,
    reserved: u32,
    held: u32,
    pending_fees: u128,
    earned_fees: u128,
    fees_in_flight: u128,
    refund_liability: u128,
}

#[near]
impl Vault {
    #[init]
    #[payable]
    pub fn new(
        registry: AccountId,
        network: String,
        denomination: U128,
        fee_recipient: AccountId,
    ) -> Self {
        let current = env::current_account_id();
        require!(
            current.get_parent_account_id() == Some(registry.as_ref())
                && env::predecessor_account_id() == registry,
            "Only the direct registry parent may initialize"
        );
        require!(
            network == "testnet" || network == "sandbox",
            "Invalid network label"
        );
        require!(
            denomination.0 > 0 && denomination.0 <= u128::MAX / 256,
            "Invalid denomination"
        );
        require!(
            fee_recipient != current && fee_recipient != registry,
            "Invalid fee recipient"
        );
        require!(
            env::attached_deposit() >= NearToken::from_near(1),
            "Attach initialization reserve"
        );
        Self {
            registry,
            network,
            denomination: denomination.0,
            fee_recipient,
            deposits: LookupMap::new(b"d"),
            payouts: LookupMap::new(b"p"),
            pending: 0,
            available: 0,
            reserved: 0,
            held: 0,
            pending_fees: 0,
            earned_fees: 0,
            fees_in_flight: 0,
            refund_liability: 0,
        }
    }

    pub fn get_config(&self) -> near_sdk::serde_json::Value {
        near_sdk::serde_json::json!({ "protocol": "core-shared-vault-v2", "registry": self.registry, "network": self.network,
            "denomination": U128(self.denomination), "fee_recipient": self.fee_recipient, "storage_fee": U128(STORAGE),
            "deposit_fee_bps": fees::DEPOSIT_FEE_BPS, "withdrawal_fee_bps": fees::WITHDRAWAL_FEE_BPS,
            "deposit_attached": U128(self.denomination + fees::deposit_fee(self.denomination) + STORAGE),
            "withdrawal_received": U128(self.denomination), "available_notes": self.available, "reserved_notes": self.reserved,
            "pending_deposits": self.pending, "held_notes": self.held, "pending_deposit_fees": U128(self.pending_fees),
            "accrued_protocol_fees": U128(self.earned_fees), "fee_payout_in_flight": U128(self.fees_in_flight), "refund_liability": U128(self.refund_liability) })
    }
    pub fn get_deposit(&self, commitment: String) -> Option<Deposit> {
        self.deposits.get(&commitment)
    }
    pub fn get_payout(&self, nullifier: String) -> Option<Payout> {
        self.payouts.get(&nullifier)
    }

    #[payable]
    pub fn deposit(&mut self, commitment: String, disclosure_root: String) -> Promise {
        field(&commitment);
        receipt(&disclosure_root);
        require!(
            commitment != "0" && self.deposits.get(&commitment).is_none(),
            "Invalid or duplicate deposit"
        );
        require!(self.held < 256, "Vault deposit capacity reached");
        require!(
            env::attached_deposit().as_yoctonear()
                == self.denomination + fees::deposit_fee(self.denomination) + STORAGE,
            "Attach principal plus deposit fee and storage"
        );
        require!(
            env::prepaid_gas() >= Gas::from_tgas(180),
            "Attach at least 180 Tgas"
        );
        let start = env::storage_usage();
        self.deposits.insert(
            &commitment,
            &Deposit {
                owner: env::predecessor_account_id(),
                disclosure_root: disclosure_root.clone(),
                state: DepositState::Pending,
                attempt: 1,
                last_height: env::block_height(),
            },
        );
        self.pending += 1;
        self.held += 1;
        self.pending_fees += fees::deposit_fee(self.denomination);
        self.storage_covered(start);
        self.register(commitment, disclosure_root, 1)
    }

    // Pending requests are resubmitted only after a delay; the registry handles them idempotently.
    #[payable]
    pub fn retry_deposit(&mut self, commitment: String) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == STORAGE,
            "Attach storage only for retry"
        );
        let mut d = self.deposits.get(&commitment).expect("Unknown deposit");
        require!(
            d.state == DepositState::Retryable
                || (d.state == DepositState::Pending
                    && env::block_height() >= d.last_height.saturating_add(RESUME_BLOCKS)),
            "Deposit not retryable yet"
        );
        d.state = DepositState::Pending;
        d.attempt += 1;
        d.last_height = env::block_height();
        self.deposits.insert(&commitment, &d);
        self.register(commitment, d.disclosure_root, d.attempt)
    }

    #[private]
    pub fn on_registered(
        &mut self,
        commitment: String,
        attempt: u64,
        #[callback_result] result: Result<bool, PromiseError>,
    ) -> bool {
        let mut d = self.deposits.get(&commitment).expect("Unknown deposit");
        if d.state == DepositState::Active {
            return true;
        }
        if d.attempt != attempt || d.state != DepositState::Pending {
            return false;
        }
        match result {
            Ok(true) => {
                self.pending -= 1;
                self.pending_fees -= fees::deposit_fee(self.denomination);
                self.available += 1;
                self.earned_fees += fees::deposit_fee(self.denomination);
                d.state = DepositState::Active;
            }
            Ok(false) => {
                self.pending -= 1;
                self.pending_fees -= fees::deposit_fee(self.denomination);
                self.refund_liability += self.denomination + fees::deposit_fee(self.denomination);
                d.state = DepositState::Refundable;
            }
            Err(_) => {
                d.state = DepositState::Retryable;
            }
        }
        self.deposits.insert(&commitment, &d);
        emit(
            "deposit_status",
            near_sdk::serde_json::json!({ "commitment": commitment, "state": d.state }),
        );
        d.state == DepositState::Active
    }

    // Registry-only execution is idempotent for the global nullifier.
    #[payable]
    pub fn payout(
        &mut self,
        nullifier: String,
        recipient: AccountId,
        disclosure_root: String,
    ) -> PromiseOrValue<bool> {
        require!(
            env::predecessor_account_id() == self.registry,
            "Only registry may authorize payouts"
        );
        require!(
            env::attached_deposit().as_yoctonear() == LOCAL_STORAGE,
            "Attach vault storage share"
        );
        field(&nullifier);
        receipt(&disclosure_root);
        require!(
            recipient != env::current_account_id() && recipient != self.registry,
            "Invalid payout recipient"
        );
        let start = env::storage_usage();
        if let Some(mut p) = self.payouts.get(&nullifier) {
            require!(
                p.recipient == recipient && p.disclosure_root == disclosure_root,
                "Payout binding mismatch"
            );
            if p.state == PayoutState::Complete {
                return PromiseOrValue::Value(true);
            }
            if p.state == PayoutState::Pending {
                return PromiseOrValue::Value(false);
            }
            p.state = PayoutState::Pending;
            self.payouts.insert(&nullifier, &p);
        } else {
            require!(self.available > 0, "Vault liquidity is not activated yet");
            self.available -= 1;
            self.reserved += 1;
            self.payouts.insert(
                &nullifier,
                &Payout {
                    recipient: recipient.clone(),
                    disclosure_root,
                    state: PayoutState::Pending,
                },
            );
        }
        self.storage_covered(start);
        self.solvent();
        Promise::new(recipient)
            .transfer(NearToken::from_yoctonear(self.denomination))
            .then(
                callback::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(20))
                    .on_payout(nullifier),
            )
            .into()
    }

    #[private]
    pub fn on_payout(
        &mut self,
        nullifier: String,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut p = self.payouts.get(&nullifier).expect("Unknown payout");
        require!(p.state == PayoutState::Pending, "Payout is not pending");
        let ok = result.is_ok();
        p.state = if ok {
            self.reserved -= 1;
            self.held -= 1;
            PayoutState::Complete
        } else {
            PayoutState::Retryable
        };
        self.payouts.insert(&nullifier, &p);
        emit(
            "vault_payout_status",
            near_sdk::serde_json::json!({ "nullifier": nullifier, "state": p.state, "recipient": p.recipient, "amount": U128(self.denomination) }),
        );
        ok
    }

    #[payable]
    pub fn claim_deposit_refund(&mut self, commitment: String) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == 1,
            "Attach 1 yoctoNEAR"
        );
        let mut d = self.deposits.get(&commitment).expect("Unknown deposit");
        require!(
            d.state == DepositState::Refundable,
            "Deposit is not refundable"
        );
        self.solvent();
        d.state = DepositState::RefundInFlight;
        self.deposits.insert(&commitment, &d);
        Promise::new(d.owner)
            .transfer(NearToken::from_yoctonear(
                self.denomination + fees::deposit_fee(self.denomination),
            ))
            .then(
                callback::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(20))
                    .on_refund(commitment),
            )
    }
    #[private]
    pub fn on_refund(
        &mut self,
        commitment: String,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut d = self.deposits.get(&commitment).expect("Unknown deposit");
        require!(
            d.state == DepositState::RefundInFlight,
            "Refund is not in flight"
        );
        let ok = result.is_ok();
        d.state = if ok {
            self.refund_liability -= self.denomination + fees::deposit_fee(self.denomination);
            self.held -= 1;
            DepositState::Refunded
        } else {
            DepositState::Refundable
        };
        self.deposits.insert(&commitment, &d);
        ok
    }

    #[payable]
    pub fn collect_protocol_fees(&mut self) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == 1,
            "Attach 1 yoctoNEAR"
        );
        require!(
            self.fees_in_flight == 0 && self.earned_fees > 0,
            "No collectible fees or collection pending"
        );
        self.solvent();
        let amount = self.earned_fees;
        self.earned_fees = 0;
        self.fees_in_flight = amount;
        Promise::new(self.fee_recipient.clone())
            .transfer(NearToken::from_yoctonear(amount))
            .then(
                callback::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(20))
                    .on_fees(U128(amount)),
            )
    }
    #[private]
    pub fn on_fees(
        &mut self,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        require!(
            amount.0 > 0 && self.fees_in_flight == amount.0,
            "Collection is not pending"
        );
        self.fees_in_flight = 0;
        if result.is_err() {
            self.earned_fees += amount.0;
        }
        result.is_ok()
    }
}

impl Vault {
    fn register(&self, commitment: String, disclosure_root: String, attempt: u64) -> Promise {
        registry::ext(self.registry.clone())
            .with_attached_deposit(NearToken::from_yoctonear(REGISTRY_STORAGE))
            .with_static_gas(Gas::from_tgas(80))
            .register_deposit(commitment.clone(), disclosure_root)
            .then(
                callback::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(25))
                    .on_registered(commitment, attempt),
            )
    }
    fn storage_covered(&self, start: u64) {
        require!(
            (env::storage_usage().saturating_sub(start) as u128 + 128)
                * env::storage_byte_cost().as_yoctonear()
                <= LOCAL_STORAGE,
            "Vault storage fee insufficient"
        );
    }
    fn solvent(&self) {
        let principal = (self.pending as u128 + self.available as u128 + self.reserved as u128)
            * self.denomination;
        let stake =
            (env::storage_usage() as u128 + 32_768) * env::storage_byte_cost().as_yoctonear();
        require!(
            env::account_balance().as_yoctonear()
                >= principal
                    + self.pending_fees
                    + self.refund_liability
                    + self.earned_fees
                    + self.fees_in_flight
                    + stake,
            "Insufficient vault reserve; retry after refunds settle"
        );
    }
}
fn receipt(value: &str) {
    require!(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "Invalid disclosure root"
    );
}
fn field(value: &str) {
    const MODULUS: &str =
        "21888242871839275222246405745257275088548364400416034343698204186575808495617";
    require!(
        !value.is_empty()
            && value.bytes().all(|c| c.is_ascii_digit())
            && (value == "0" || !value.starts_with('0'))
            && (value.len() < MODULUS.len() || (value.len() == MODULUS.len() && value < MODULUS)),
        "Invalid field encoding"
    );
}
fn emit(event: &str, data: near_sdk::serde_json::Value) {
    env::log_str(&format!(
        "EVENT_JSON:{}",
        near_sdk::serde_json::json!({ "standard": "core_shared", "version": "2.0.0", "event": event, "data": [data] })
    ));
}

#[cfg(test)]
mod tests;
