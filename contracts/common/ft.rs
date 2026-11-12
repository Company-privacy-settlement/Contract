//! Shared NEP-141 calls and separately funded NEAR storage credits.
use near_sdk::collections::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, Promise};

pub const STORAGE_CREDIT_BOND: u128 = 5_000_000_000_000_000_000_000; // 0.005 NEAR, once per payer per contract.
pub const ACTION_STORAGE: u128 = 20_000_000_000_000_000_000_000;
#[near(serializers = [borsh, json])]
#[derive(Clone, Default)]
pub struct Credit {
    pub available: U128,
    pub in_flight: U128,
}
#[near(serializers = [borsh])]
pub struct StorageCredits {
    balances: LookupMap<AccountId, Credit>,
    pub liability: u128,
}
impl StorageCredits {
    pub fn new() -> Self {
        Self {
            balances: LookupMap::new(b"FT:storage".as_slice()),
            liability: 0,
        }
    }
    pub fn view(&self, account: &AccountId) -> Option<Credit> {
        self.balances.get(account)
    }
    pub fn get(&self, account: &AccountId) -> Credit {
        self.balances.get(account).unwrap_or_default()
    }
    pub fn prepay(&mut self, account: AccountId) -> Credit {
        let start = env::storage_usage();
        let existing = self.balances.get(&account);
        let bond = if existing.is_none() {
            STORAGE_CREDIT_BOND
        } else {
            0
        };
        let attached = env::attached_deposit().as_yoctonear();
        require!(
            attached >= bond + if bond > 0 { ACTION_STORAGE } else { 1 },
            "Attach action storage plus first-use credit bond"
        );
        let mut credit = existing.unwrap_or_default();
        let added = attached - bond;
        credit.available.0 = credit
            .available
            .0
            .checked_add(added)
            .expect("Storage credit overflow");
        self.liability = self
            .liability
            .checked_add(added)
            .expect("Storage liability overflow");
        self.balances.insert(&account, &credit);
        require!(
            (env::storage_usage().saturating_sub(start) as u128)
                * env::storage_byte_cost().as_yoctonear()
                <= bond,
            "Credit bond insufficient"
        );
        credit
    }
    pub fn consume(&mut self, account: &AccountId) {
        let mut credit = self.get(account);
        require!(
            credit.available.0 >= ACTION_STORAGE,
            "Prepay NEAR storage before transferring tokens"
        );
        credit.available.0 -= ACTION_STORAGE;
        self.liability -= ACTION_STORAGE;
        self.balances.insert(account, &credit);
    }
    pub fn withdraw(&mut self) -> Promise {
        require!(
            env::attached_deposit().as_yoctonear() == 1,
            "Attach 1 yoctoNEAR"
        );
        let account = env::predecessor_account_id();
        let mut credit = self.get(&account);
        require!(
            credit.available.0 > 0 && credit.in_flight.0 == 0,
            "No available credit or refund pending"
        );
        let amount = credit.available;
        credit.available = U128(0);
        credit.in_flight = amount;
        self.balances.insert(&account, &credit);
        Promise::new(account.clone())
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "on_storage_refund",
                    near_sdk::serde_json::to_vec(
                        &near_sdk::serde_json::json!({"account": account, "amount": amount}),
                    )
                    .unwrap(),
                    NearToken::from_yoctonear(0),
                    Gas::from_tgas(15),
                ),
            )
    }
    pub fn resolve(&mut self, account: AccountId, amount: U128, ok: bool) -> bool {
        let mut credit = self.get(&account);
        require!(
            amount.0 > 0 && credit.in_flight == amount,
            "Storage refund not pending"
        );
        credit.in_flight = U128(0);
        if ok {
            self.liability -= amount.0;
        } else {
            credit.available.0 += amount.0;
        }
        self.balances.insert(&account, &credit);
        ok
    }
}
pub fn send(token: &Option<AccountId>, recipient: AccountId, amount: u128) -> Promise {
    if let Some(token_id) = token {
        Promise::new(token_id.clone()).function_call("ft_transfer",
            near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({"receiver_id": recipient, "amount": U128(amount), "memo": null})).unwrap(),
            NearToken::from_yoctonear(1), Gas::from_tgas(25))
    } else {
        Promise::new(recipient).transfer(NearToken::from_yoctonear(amount))
    }
}
