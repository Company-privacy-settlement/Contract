use near_sdk::collections::{LookupMap, LookupSet, Vector};
#[path = "../../pool/src/crypto.rs"]
mod crypto;
use crypto::{decimal, parse, poseidon, sha_field, Field};
#[path = "../../pool/src/fees.rs"]
mod fees;
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::{
    env, ext_contract, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise,
    PromiseError,
};

// Built from the pool source before compiling the factory.
// The bytes and verifier are pinned per factory; no setter or upgrade method.
const VAULT_WASM: &[u8] = include_bytes!("../../../artifacts/core_shared_vault.wasm");
const REGISTRATION_FEE: u128 = 50_000_000_000_000_000_000_000; // 0.05 NEAR
const INIT_RESERVE: u128 = 1_000_000_000_000_000_000_000_000; // 1 NEAR
const STATE_BUFFER: u128 = 65_536;
const STORAGE: u128 = 20_000_000_000_000_000_000_000;
const LOCAL_STORAGE: u128 = STORAGE / 4;
const REGISTRY_STORAGE: u128 = STORAGE - LOCAL_STORAGE;
const DEPTH: usize = 8;
const CAPACITY: u32 = 1 << DEPTH;
const ZERO: Field = [0; 32];
const RESUME_BLOCKS: u64 = 20;

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum VaultState {
    Pending,
    Ready,
    Failed,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct VaultRecord {
    pub index: u64,
    pub account_id: AccountId,
    pub creator: AccountId,
    pub state: VaultState,
    pub funding: U128,
    pub available_notes: u32,
    pub reserved_notes: u32,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Default)]
pub struct RefundBalance {
    pub available: U128,
    pub in_flight: U128,
}

#[near(serializers = [json])]
pub struct ForestConfig {
    pub protocol: String,
    pub network: String,
    pub denomination: U128,
    pub fee_recipient: AccountId,
    pub deposit_fee_bps: u16,
    pub withdrawal_fee_bps: u16,
    pub deposit_protocol_fee: U128,
    pub withdrawal_protocol_fee: U128,
    pub vault_wasm_sha256: String,
    pub verifier_sha256: String,
    pub vault_funding: U128,
    pub registration_fee: U128,
    pub creation_deposit: U128,
    pub vault_count: u64,
    pub pending_funding: U128,
    pub refund_liability: U128,
    pub domain: String,
    pub root: String,
    pub leaf_count: u32,
    pub capacity: u32,
    pub outstanding_notes: u32,
    pub record_count: u64,
    pub storage_fee: U128,
    pub deposit_attached: U128,
    pub withdrawal_attached: U128,
    pub withdrawal_received: U128,
    pub pending_withdrawal_fees: U128,
    pub accrued_protocol_fees: U128,
    pub fee_payout_in_flight: U128,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum WithdrawalState {
    Pending,
    Retryable,
    Complete,
}
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Withdrawal {
    pub vault: AccountId,
    pub recipient: AccountId,
    pub disclosure_root: String,
    pub state: WithdrawalState,
    pub attempt: u64,
    pub last_height: u64,
}
#[near(serializers = [borsh])]
pub struct Registration {
    vault: AccountId,
    disclosure_root: String,
}
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Record {
    pub index: u64,
    pub kind: String,
    pub commitment: Option<String>,
    pub nullifier: Option<String>,
    pub vault: Option<AccountId>,
    pub recipient: Option<AccountId>,
    pub disclosure_root: String,
    pub block_height: u64,
    pub timestamp_ns: String,
    pub amount: U128,
    pub protocol_fee_charged: U128,
    pub protocol_fee_earned: U128,
    pub storage_fee_charged: U128,
}

#[ext_contract(vault_contract)]
#[allow(dead_code)]
trait VaultContract {
    fn payout(&mut self, nullifier: String, recipient: AccountId, disclosure_root: String) -> bool;
}

#[ext_contract(ext_self)]
#[allow(dead_code)]
trait Callbacks {
    fn on_dispatched(
        &mut self,
        nullifier: String,
        attempt: u64,
        #[callback_result] result: Result<bool, PromiseError>,
    ) -> bool;
    fn on_fees(
        &mut self,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool;
    fn on_created(
        &mut self,
        index: u64,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool;
    fn on_refund(
        &mut self,
        creator: AccountId,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Forest {
    network: String,
    denomination: u128,
    fee_recipient: AccountId,
    verification_key: Vec<u8>,
    funding: u128,
    vaults: Vector<VaultRecord>,
    indexes: LookupMap<AccountId, u64>,
    refunds: LookupMap<AccountId, RefundBalance>,
    pending_funding: u128,
    refund_liability: u128,
    domain: Field,
    nodes: Vec<Field>,
    leaves: Vector<Field>,
    roots: LookupSet<Field>,
    commitments: LookupMap<Field, Registration>,
    spent: LookupSet<Field>,
    withdrawals: LookupMap<Field, Withdrawal>,
    records: Vector<Record>,
    outstanding: u32,
    pending_fees: u128,
    earned_fees: u128,
    fees_in_flight: u128,
}

#[near]
impl Forest {
    #[init]
    #[payable]
    pub fn new(
        network: String,
        denomination: U128,
        verification_key: Base64VecU8,
        fee_recipient: AccountId,
    ) -> Self {
        require!(
            fee_recipient != env::current_account_id(),
            "Fee recipient cannot be the factory itself"
        );
        require!(
            env::predecessor_account_id() == env::current_account_id(),
            "Initialize from factory account"
        );
        require!(
            env::current_account_id().as_str().len() <= 30,
            "Forest account ID exceeds 30 characters"
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
            verification_key.0.len() == 832,
            "Expected five-input Groth16 key"
        );
        require!(
            env::attached_deposit().as_yoctonear() >= INIT_RESERVE,
            "Attach 1 NEAR factory reserve"
        );
        let funding = (VAULT_WASM.len() as u128 + STATE_BUFFER)
            * env::storage_byte_cost().as_yoctonear()
            + INIT_RESERVE;
        crypto::validate_vk(&verification_key.0);
        let domain = sha_field(&format!(
            "core:v2|{}|{}|{}",
            network,
            env::current_account_id(),
            denomination.0
        ));
        let mut nodes = vec![ZERO; 2 * CAPACITY as usize - 1];
        let mut zero = ZERO;
        for level in (0..DEPTH).rev() {
            zero = poseidon(&zero, &zero);
            nodes[(1 << level) - 1..(1 << (level + 1)) - 1].fill(zero);
        }
        let mut roots = LookupSet::new(b"R");
        roots.insert(&nodes[0]);
        Self {
            network,
            denomination: denomination.0,
            fee_recipient,
            verification_key: verification_key.0,
            funding,
            vaults: Vector::new(b"v"),
            indexes: LookupMap::new(b"i"),
            refunds: LookupMap::new(b"r"),
            pending_funding: 0,
            refund_liability: 0,
            domain,
            nodes,
            leaves: Vector::new(b"L"),
            roots,
            commitments: LookupMap::new(b"C"),
            spent: LookupSet::new(b"S"),
            withdrawals: LookupMap::new(b"W"),
            records: Vector::new(b"E"),
            outstanding: 0,
            pending_fees: 0,
            earned_fees: 0,
            fees_in_flight: 0,
        }
    }

    pub fn get_config(&self) -> ForestConfig {
        ForestConfig {
            protocol: "core-shared-registry-v2".into(),
            network: self.network.clone(),
            denomination: U128(self.denomination),
            fee_recipient: self.fee_recipient.clone(),
            deposit_fee_bps: fees::DEPOSIT_FEE_BPS,
            withdrawal_fee_bps: fees::WITHDRAWAL_FEE_BPS,
            deposit_protocol_fee: U128(fees::deposit_fee(self.denomination)),
            withdrawal_protocol_fee: U128(fees::withdrawal_fee(self.denomination)),
            vault_wasm_sha256: hex::encode(env::sha256(VAULT_WASM)),
            verifier_sha256: hex::encode(env::sha256(&self.verification_key)),
            vault_funding: U128(self.funding),
            registration_fee: U128(REGISTRATION_FEE),
            creation_deposit: U128(self.funding + REGISTRATION_FEE),
            vault_count: self.vaults.len(),
            pending_funding: U128(self.pending_funding),
            refund_liability: U128(self.refund_liability),
            domain: decimal(&self.domain),
            root: decimal(&self.nodes[0]),
            leaf_count: self.leaves.len() as u32,
            capacity: CAPACITY,
            outstanding_notes: self.outstanding,
            record_count: self.records.len(),
            storage_fee: U128(STORAGE),
            deposit_attached: U128(
                self.denomination + fees::deposit_fee(self.denomination) + STORAGE,
            ),
            withdrawal_attached: U128(fees::withdrawal_fee(self.denomination) + STORAGE),
            withdrawal_received: U128(self.denomination),
            pending_withdrawal_fees: U128(self.pending_fees),
            accrued_protocol_fees: U128(self.earned_fees),
            fee_payout_in_flight: U128(self.fees_in_flight),
        }
    }

    pub fn predict_vault(&self, creator: AccountId, salt: String) -> AccountId {
        require!(
            salt.len() == 64
                && salt
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Salt must be 32-byte lowercase hex"
        );
        let digest = hex::encode(env::sha256(
            format!("core:shared-vault:v2|{}|{}", creator, salt).as_bytes(),
        ));
        format!("v{}.{}", &digest[..32], env::current_account_id())
            .parse()
            .expect("Valid child account")
    }

    pub fn list_vaults(&self, from_index: u64, limit: u32) -> Vec<VaultRecord> {
        (from_index
            ..from_index
                .saturating_add(limit.min(50) as u64)
                .min(self.vaults.len()))
            .map(|index| self.vaults.get(index).expect("Vault record"))
            .collect()
    }

    pub fn get_vault(&self, account_id: AccountId) -> Option<VaultRecord> {
        self.indexes
            .get(&account_id)
            .and_then(|index| self.vaults.get(index))
    }

    pub fn get_refund(&self, creator: AccountId) -> RefundBalance {
        self.refunds.get(&creator).unwrap_or_default()
    }

    // Only bootstrapping funds pass through here. Settlement deposits go directly to vaults.
    #[payable]
    pub fn create_vault(&mut self, salt: String) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == self.funding + REGISTRATION_FEE,
            "Attach exact creation deposit from get_config"
        );
        require!(
            env::prepaid_gas() >= Gas::from_tgas(220),
            "Attach at least 220 Tgas"
        );
        let creator = env::predecessor_account_id();
        let account_id = self.predict_vault(creator.clone(), salt);
        require!(
            self.indexes.get(&account_id).is_none(),
            "Creation already requested; use a fresh salt"
        );
        let index = self.vaults.len();
        let start = env::storage_usage();
        self.indexes.insert(&account_id, &index);
        self.vaults.push(&VaultRecord {
            index,
            account_id: account_id.clone(),
            creator,
            state: VaultState::Pending,
            funding: U128(self.funding),
            available_notes: 0,
            reserved_notes: 0,
        });
        self.pending_funding += self.funding;
        require!(
            (env::storage_usage().saturating_sub(start) as u128 + 1024)
                * env::storage_byte_cost().as_yoctonear()
                <= REGISTRATION_FEE,
            "Registration fee does not cover storage"
        );
        self.emit("vault_requested", index);
        let args = near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({
            "network": self.network, "denomination": U128(self.denomination), "registry": env::current_account_id(), "fee_recipient": self.fee_recipient
        })).expect("Initialization arguments");
        // Never add any full-access or function-call key to the child.
        Promise::new(account_id)
            .create_account()
            .transfer(NearToken::from_yoctonear(self.funding - INIT_RESERVE))
            .deploy_contract(VAULT_WASM.to_vec())
            .function_call(
                "new",
                args,
                NearToken::from_yoctonear(INIT_RESERVE),
                Gas::from_tgas(80),
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(25))
                    .on_created(index),
            )
    }

    #[private]
    pub fn on_created(
        &mut self,
        index: u64,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut record = self.vaults.get(index).expect("Unknown creation");
        require!(
            record.state == VaultState::Pending,
            "Creation is not pending"
        );
        self.pending_funding -= record.funding.0;
        let ok = result.is_ok();
        record.state = if ok {
            VaultState::Ready
        } else {
            VaultState::Failed
        };
        if !ok {
            let mut refund = self.get_refund(record.creator.clone());
            refund.available.0 += record.funding.0;
            self.refund_liability += record.funding.0;
            self.refunds.insert(&record.creator, &refund);
        }
        self.vaults.replace(index, &record);
        self.emit(if ok { "vault_ready" } else { "vault_failed" }, index);
        ok
    }

    // Pull refunds: an automatic protocol refund may arrive after on_created.
    // Destination is always the original creator, never a caller-supplied address.
    #[payable]
    pub fn claim_refund(&mut self) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == 1,
            "Attach 1 yoctoNEAR"
        );
        let creator = env::predecessor_account_id();
        let mut refund = self.get_refund(creator.clone());
        require!(refund.available.0 > 0, "No available refund");
        require!(refund.in_flight.0 == 0, "Refund is already in flight");
        let stake =
            (env::storage_usage() as u128 + STATE_BUFFER) * env::storage_byte_cost().as_yoctonear();
        require!(
            env::account_balance().as_yoctonear()
                >= stake
                    + self.refund_liability
                    + self.pending_funding
                    + self.pending_fees
                    + self.earned_fees
                    + self.fees_in_flight,
            "Protocol refunds have not settled; retry later"
        );
        let amount = refund.available;
        refund.available = U128(0);
        refund.in_flight = amount;
        self.refunds.insert(&creator, &refund);
        Promise::new(creator.clone())
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(15))
                    .on_refund(creator, amount),
            )
    }

    #[private]
    pub fn on_refund(
        &mut self,
        creator: AccountId,
        amount: U128,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let mut refund = self.get_refund(creator.clone());
        require!(
            amount.0 > 0 && refund.in_flight == amount,
            "Refund is not in flight"
        );
        refund.in_flight = U128(0);
        let ok = result.is_ok();
        if ok {
            self.refund_liability -= amount.0;
        } else {
            refund.available.0 += amount.0;
        }
        self.refunds.insert(&creator, &refund);
        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::json!({
                "standard": "core_factory", "version": "1.0.0", "event": if ok { "refund_succeeded" } else { "refund_failed" },
                "data": [{ "creator": creator, "amount": amount }]
            })
        ));
        ok
    }
}

#[near]
impl Forest {
    pub fn get_commitments(&self, from_index: u64, limit: u32) -> Vec<String> {
        (from_index
            ..from_index
                .saturating_add(limit.min(256) as u64)
                .min(self.leaves.len()))
            .map(|i| decimal(&self.leaves.get(i).expect("Leaf")))
            .collect()
    }
    pub fn get_records(&self, from_index: u64, limit: u32) -> Vec<Record> {
        (from_index
            ..from_index
                .saturating_add(limit.min(50) as u64)
                .min(self.records.len()))
            .map(|i| self.records.get(i).expect("Record"))
            .collect()
    }
    pub fn is_spent(&self, nullifier: String) -> bool {
        self.spent.contains(&parse(&nullifier))
    }
    pub fn get_withdrawal(&self, nullifier: String) -> Option<Withdrawal> {
        self.withdrawals.get(&parse(&nullifier))
    }

    // The source vault keeps all principal. Only authenticated, pinned-code children may register it.
    #[payable]
    pub fn register_deposit(&mut self, commitment: String, disclosure_root: String) -> bool {
        require!(
            env::attached_deposit().as_yoctonear() == REGISTRY_STORAGE,
            "Attach registry storage share"
        );
        let vault = env::predecessor_account_id();
        let (index, mut v) = self.ready_vault(&vault);
        Self::receipt(&disclosure_root);
        let c = parse(&commitment);
        require!(c != ZERO, "Empty commitment");
        if let Some(previous) = self.commitments.get(&c) {
            return previous.vault == vault && previous.disclosure_root == disclosure_root;
        }
        if self.leaves.len() >= CAPACITY as u64 {
            return false;
        }
        let start = env::storage_usage();
        self.append(
            c,
            Registration {
                vault: vault.clone(),
                disclosure_root: disclosure_root.clone(),
            },
        );
        v.available_notes += 1;
        self.vaults.replace(index, &v);
        self.outstanding += 1;
        self.record(
            "deposit",
            Some(commitment),
            None,
            Some(vault),
            None,
            disclosure_root,
        );
        self.storage_covered(start, REGISTRY_STORAGE, 256);
        true
    }

    #[payable]
    pub fn transfer(
        &mut self,
        output_commitment: String,
        disclosure_root: String,
        proof: Base64VecU8,
        public_inputs: Vec<String>,
    ) -> u64 {
        require!(
            env::attached_deposit().as_yoctonear() == STORAGE,
            "Attach storage fee"
        );
        Self::receipt(&disclosure_root);
        let output = parse(&output_commitment);
        require!(
            output != ZERO
                && self.commitments.get(&output).is_none()
                && self.leaves.len() < CAPACITY as u64,
            "Invalid output or full tree"
        );
        let nf = self.check_spend(
            "transfer",
            "",
            &output_commitment,
            &disclosure_root,
            &proof.0,
            &public_inputs,
        );
        let start = env::storage_usage();
        self.spent.insert(&nf);
        let index = self.append(
            output,
            Registration {
                vault: env::current_account_id(),
                disclosure_root: disclosure_root.clone(),
            },
        );
        self.record(
            "transfer",
            Some(output_commitment),
            Some(decimal(&nf)),
            None,
            None,
            disclosure_root,
        );
        self.storage_covered(start, STORAGE, 0);
        index
    }

    #[payable]
    pub fn withdraw(
        &mut self,
        vault: AccountId,
        recipient: AccountId,
        disclosure_root: String,
        proof: Base64VecU8,
        public_inputs: Vec<String>,
    ) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear()
                == fees::withdrawal_fee(self.denomination) + STORAGE,
            "Attach add-on withdrawal fee and storage"
        );
        require!(
            env::prepaid_gas() >= Gas::from_tgas(240),
            "Attach at least 240 Tgas"
        );
        let (index, mut v) = self.ready_vault(&vault);
        require!(
            v.available_notes > 0,
            "Selected vault has no available liquidity"
        );
        require!(
            recipient != env::current_account_id() && self.indexes.get(&recipient).is_none(),
            "Recipient cannot be the registry or a vault"
        );
        Self::receipt(&disclosure_root);
        let nf = self.check_spend(
            "withdraw",
            vault.as_str(),
            recipient.as_str(),
            &disclosure_root,
            &proof.0,
            &public_inputs,
        );
        let start = env::storage_usage();
        self.spent.insert(&nf);
        v.available_notes -= 1;
        v.reserved_notes += 1;
        self.vaults.replace(index, &v);
        self.pending_fees += fees::withdrawal_fee(self.denomination);
        let w = Withdrawal {
            vault: vault.clone(),
            recipient: recipient.clone(),
            disclosure_root: disclosure_root.clone(),
            state: WithdrawalState::Pending,
            attempt: 1,
            last_height: env::block_height(),
        };
        self.withdrawals.insert(&nf, &w);
        self.record(
            "withdrawal_requested",
            None,
            Some(decimal(&nf)),
            Some(vault),
            Some(recipient),
            disclosure_root,
        );
        self.storage_covered(start, REGISTRY_STORAGE, 384);
        self.dispatch(nf, w)
    }

    // Never releases the nullifier or redirects payout. A lost callback can be resumed idempotently.
    #[payable]
    pub fn retry_withdrawal(&mut self, nullifier: String) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == STORAGE,
            "Attach storage only for retry"
        );
        let nf = parse(&nullifier);
        let mut w = self.withdrawals.get(&nf).expect("Unknown withdrawal");
        require!(
            w.state == WithdrawalState::Retryable
                || (w.state == WithdrawalState::Pending
                    && env::block_height() >= w.last_height.saturating_add(RESUME_BLOCKS)),
            "Withdrawal not retryable yet"
        );
        let start = env::storage_usage();
        w.state = WithdrawalState::Pending;
        w.attempt += 1;
        w.last_height = env::block_height();
        self.withdrawals.insert(&nf, &w);
        self.record(
            "withdrawal_retried",
            None,
            Some(nullifier),
            Some(w.vault.clone()),
            Some(w.recipient.clone()),
            w.disclosure_root.clone(),
        );
        self.storage_covered(start, REGISTRY_STORAGE, 384);
        self.dispatch(nf, w)
    }

    #[private]
    pub fn on_dispatched(
        &mut self,
        nullifier: String,
        attempt: u64,
        #[callback_result] result: Result<bool, PromiseError>,
    ) -> bool {
        let nf = parse(&nullifier);
        let mut w = self.withdrawals.get(&nf).expect("Unknown withdrawal");
        if w.state == WithdrawalState::Complete {
            return true;
        }
        if w.attempt != attempt || w.state != WithdrawalState::Pending {
            return false;
        }
        let ok = matches!(result, Ok(true));
        w.state = if ok {
            WithdrawalState::Complete
        } else {
            WithdrawalState::Retryable
        };
        if ok {
            let (index, mut v) = self.ready_vault(&w.vault);
            v.reserved_notes -= 1;
            self.vaults.replace(index, &v);
            self.outstanding -= 1;
            let fee = fees::withdrawal_fee(self.denomination);
            self.pending_fees -= fee;
            self.earned_fees += fee;
        }
        self.withdrawals.insert(&nf, &w);
        self.record(
            if ok {
                "withdrawal_succeeded"
            } else {
                "withdrawal_failed"
            },
            None,
            Some(nullifier),
            Some(w.vault),
            Some(w.recipient),
            w.disclosure_root,
        );
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
                ext_self::ext(env::current_account_id())
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
            "Collection not pending"
        );
        self.fees_in_flight = 0;
        if result.is_err() {
            self.earned_fees += amount.0;
        }
        result.is_ok()
    }
}

impl Forest {
    fn ready_vault(&self, account: &AccountId) -> (u64, VaultRecord) {
        let index = self.indexes.get(account).expect("Unregistered vault");
        let record = self.vaults.get(index).expect("Vault");
        require!(record.state == VaultState::Ready, "Vault is not ready");
        (index, record)
    }
    fn append(&mut self, value: Field, registration: Registration) -> u64 {
        let leaf = self.leaves.len();
        require!(
            leaf < CAPACITY as u64 && self.commitments.get(&value).is_none(),
            "Tree full or duplicate output"
        );
        self.commitments.insert(&value, &registration);
        self.leaves.push(&value);
        let mut i = CAPACITY as usize - 1 + leaf as usize;
        self.nodes[i] = value;
        while i > 0 {
            i = (i - 1) / 2;
            self.nodes[i] = poseidon(&self.nodes[2 * i + 1], &self.nodes[2 * i + 2]);
        }
        self.roots.insert(&self.nodes[0]);
        leaf
    }
    fn check_spend(
        &self,
        kind: &str,
        vault: &str,
        target: &str,
        disclosure: &str,
        proof: &[u8],
        public_inputs: &[String],
    ) -> Field {
        require!(public_inputs.len() == 5, "Expected five public inputs");
        let fields: Vec<Field> = public_inputs.iter().map(|s| parse(s)).collect();
        require!(fields[4] == self.domain, "Wrong shared domain");
        require!(self.roots.contains(&fields[2]), "Unknown global root");
        let context = sha_field(&format!(
            "core:action:v2|{}|{}|{}|{}|{}",
            decimal(&self.domain),
            kind,
            vault,
            target,
            disclosure
        ));
        require!(
            fields[3] == context,
            "Wrong action or destination vault binding"
        );
        require!(
            !self.spent.contains(&fields[0]),
            "Global nullifier already spent"
        );
        require!(
            crypto::verify(&self.verification_key, proof, &fields),
            "Invalid Groth16 proof"
        );
        fields[0]
    }
    fn dispatch(&self, nf: Field, w: Withdrawal) -> Promise {
        vault_contract::ext(w.vault)
            .with_attached_deposit(NearToken::from_yoctonear(LOCAL_STORAGE))
            .with_static_gas(Gas::from_tgas(80))
            .payout(decimal(&nf), w.recipient, w.disclosure_root)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(25))
                    .on_dispatched(decimal(&nf), w.attempt),
            )
    }
    fn storage_covered(&self, start: u64, budget: u128, future_bytes: u128) {
        require!(
            (env::storage_usage().saturating_sub(start) as u128 + future_bytes)
                * env::storage_byte_cost().as_yoctonear()
                <= budget,
            "Registry storage fee insufficient"
        );
    }
    fn solvent(&self) {
        let stake =
            (env::storage_usage() as u128 + STATE_BUFFER) * env::storage_byte_cost().as_yoctonear();
        require!(
            env::account_balance().as_yoctonear()
                >= stake
                    + self.refund_liability
                    + self.pending_funding
                    + self.pending_fees
                    + self.earned_fees
                    + self.fees_in_flight,
            "Insufficient registry reserve; retry after refunds settle"
        );
    }
    fn receipt(s: &str) {
        require!(
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "Invalid disclosure root"
        );
    }
    fn record(
        &mut self,
        kind: &str,
        commitment: Option<String>,
        nullifier: Option<String>,
        vault: Option<AccountId>,
        recipient: Option<AccountId>,
        disclosure_root: String,
    ) {
        let (charged, earned, storage) = match kind {
            "deposit" => {
                let fee = fees::deposit_fee(self.denomination);
                (fee, fee, STORAGE)
            }
            "withdrawal_requested" => (fees::withdrawal_fee(self.denomination), 0, STORAGE),
            "withdrawal_succeeded" => (0, fees::withdrawal_fee(self.denomination), 0),
            "transfer" | "withdrawal_retried" => (0, 0, STORAGE),
            _ => (0, 0, 0),
        };
        let record = Record {
            index: self.records.len(),
            kind: kind.into(),
            commitment,
            nullifier,
            vault,
            recipient,
            disclosure_root,
            block_height: env::block_height(),
            timestamp_ns: env::block_timestamp().to_string(),
            amount: U128(self.denomination),
            protocol_fee_charged: U128(charged),
            protocol_fee_earned: U128(earned),
            storage_fee_charged: U128(storage),
        };
        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::json!({ "standard": "core_shared", "version": "2.0.0", "event": kind, "data": [&record] })
        ));
        self.records.push(&record);
    }
    fn emit(&self, kind: &str, index: u64) {
        let record = self.vaults.get(index).expect("Vault record");
        env::log_str(&format!(
            "EVENT_JSON:{}",
            near_sdk::serde_json::json!({
                "standard": "core_factory", "version": "1.0.0", "event": kind, "data": [&record]
            })
        ));
    }
}

#[cfg(test)]
mod tests;
