use super::*;
use near_sdk::serde_json::{self, Value};
use near_sdk::{
    mock::MockAction,
    test_utils::{get_created_receipts, VMContextBuilder},
    testing_env,
};
const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
fn vector() -> Value {
    serde_json::from_str(include_str!("../../../test-vectors/shared.json")).unwrap()
}
fn s(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap().into()
}
fn context(caller: &str, deposit: u128, height: u64) {
    let mut b = VMContextBuilder::new();
    b.current_account_id("forest.test.near".parse().unwrap())
        .predecessor_account_id(caller.parse().unwrap())
        .signer_account_id(caller.parse().unwrap())
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000))
        .prepaid_gas(Gas::from_tgas(300))
        .block_height(height);
    testing_env!(b.build());
}
fn init(v: &Value) -> Forest {
    context("forest.test.near", NEAR, 0);
    let mut f = Forest::new(
        "sandbox".into(),
        U128(NEAR),
        serde_json::from_value(v["verificationKey"].clone()).unwrap(),
        "treasury.test.near".parse().unwrap(),
    );
    for key in ["vaultA", "vaultB"] {
        let account = s(v, key).parse().unwrap();
        let index = f.vaults.len();
        f.indexes.insert(&account, &index);
        f.vaults.push(&VaultRecord {
            index,
            account_id: account,
            creator: "alice.test.near".parse().unwrap(),
            state: VaultState::Ready,
            funding: U128(f.funding),
            available_notes: 0,
            reserved_notes: 0,
        });
    }
    f
}
fn funded(v: &Value) -> Forest {
    let mut f = init(v);
    context(&s(v, "vaultA"), REGISTRY_STORAGE, 1);
    assert!(f.register_deposit(s(v, "a"), "a1".repeat(32)));
    context(&s(v, "vaultB"), REGISTRY_STORAGE, 2);
    assert!(f.register_deposit(s(v, "b"), "b2".repeat(32)));
    assert_eq!(f.get_config().root, s(v, "root"));
    assert_eq!(f.get_config().domain, s(v, "domain"));
    f
}
fn proof(v: &Value, key: &str) -> (Base64VecU8, Vec<String>) {
    (
        serde_json::from_value(v["proofs"][key]["proof"].clone()).unwrap(),
        serde_json::from_value(v["proofs"][key]["public_inputs"].clone()).unwrap(),
    )
}
fn withdraw(f: &mut Forest, v: &Value, name: &str, target: &str) -> String {
    let (p, inputs) = proof(v, name);
    let nf = inputs[0].clone();
    context("relay.test.near", STORAGE + fees::withdrawal_fee(NEAR), 5);
    f.withdraw(
        s(v, target).parse().unwrap(),
        s(v, "recipient").parse().unwrap(),
        s(v, "disclosureRoot"),
        p,
        inputs,
    )
    .detach();
    nf
}

#[test]
fn global_membership_spends_an_a_deposit_from_b_without_moving_principal_to_registry() {
    let v = vector();
    let mut f = funded(&v);
    let nf = withdraw(&mut f, &v, "aFromB", "vaultB");
    assert!(f.is_spent(nf.clone()));
    assert_eq!(
        f.get_vault(s(&v, "vaultA").parse().unwrap())
            .unwrap()
            .available_notes,
        1
    );
    assert_eq!(
        f.get_vault(s(&v, "vaultB").parse().unwrap())
            .unwrap()
            .reserved_notes,
        1
    );
    let receipts = get_created_receipts();
    assert!(receipts.iter().any(|r| r.receiver_id.as_str() == s(&v, "vaultB") && r.actions.iter().any(|a|
        matches!(a, MockAction::FunctionCallWeight { attached_deposit, method_name, .. } if method_name == b"payout" && attached_deposit.as_yoctonear() == LOCAL_STORAGE))));
    assert!(!receipts.iter().flat_map(|r| &r.actions).any(
        |a| matches!(a, MockAction::Transfer { deposit, .. } if deposit.as_yoctonear() >= NEAR)
    ));
    context("forest.test.near", 0, 6);
    assert!(f.on_dispatched(nf.clone(), 1, Ok(true)));
    assert_eq!(f.outstanding, 1);
    assert_eq!(f.earned_fees, fees::withdrawal_fee(NEAR));
    assert!(f.on_dispatched(nf, 1, Ok(true)));
    assert_eq!(f.outstanding, 1);
}
#[test]
#[should_panic(expected = "Global nullifier already spent")]
fn two_valid_destination_proofs_for_one_note_cannot_spend_twice() {
    let v = vector();
    let mut f = funded(&v);
    withdraw(&mut f, &v, "aFromB", "vaultB");
    withdraw(&mut f, &v, "aFromA", "vaultA");
}
#[test]
#[should_panic(expected = "Wrong action or destination vault binding")]
fn relayer_cannot_replace_the_proven_destination_vault() {
    let v = vector();
    let mut f = funded(&v);
    withdraw(&mut f, &v, "aFromB", "vaultA");
}
#[test]
fn failures_keep_nullifier_and_liquidity_reserved_and_ignore_stale_callbacks() {
    let v = vector();
    let mut f = funded(&v);
    let nf = withdraw(&mut f, &v, "aFromB", "vaultB");
    context("forest.test.near", 0, 6);
    assert!(!f.on_dispatched(nf.clone(), 1, Err(PromiseError::Failed)));
    assert!(f.is_spent(nf.clone()));
    assert_eq!(f.pending_fees, fees::withdrawal_fee(NEAR));
    assert_eq!(f.earned_fees, 0);
    context("anyone.test.near", STORAGE, 7);
    f.retry_withdrawal(nf.clone()).detach();
    context("forest.test.near", 0, 8);
    assert!(!f.on_dispatched(nf.clone(), 1, Ok(true)));
    assert_eq!(f.outstanding, 2);
    assert!(f.on_dispatched(nf.clone(), 2, Ok(true)));
    assert_eq!(f.outstanding, 1);
    assert_eq!(f.pending_fees, 0);
    assert_eq!(f.earned_fees, fees::withdrawal_fee(NEAR));
    assert_eq!(
        f.get_withdrawal(nf).unwrap().vault.as_str(),
        s(&v, "vaultB")
    );
}
#[test]
fn lost_callback_can_be_resumed_after_delay_without_new_protocol_fee() {
    let v = vector();
    let mut f = funded(&v);
    let nf = withdraw(&mut f, &v, "aFromB", "vaultB");
    context("anyone.test.near", STORAGE, 25);
    f.retry_withdrawal(nf.clone()).detach();
    context("forest.test.near", 0, 26);
    assert!(f.on_dispatched(nf, 2, Ok(true)));
    assert_eq!(f.outstanding, 1);
    assert_eq!(f.earned_fees, fees::withdrawal_fee(NEAR));
}
#[test]
fn registration_retries_do_not_mint_extra_notes_or_vault_liquidity() {
    let v = vector();
    let mut f = funded(&v);
    context(&s(&v, "vaultA"), REGISTRY_STORAGE, 3);
    assert!(f.register_deposit(s(&v, "a"), "a1".repeat(32)));
    assert_eq!(f.leaves.len(), 2);
    assert_eq!(f.outstanding, 2);
    context(&s(&v, "vaultB"), REGISTRY_STORAGE, 4);
    assert!(!f.register_deposit(s(&v, "a"), "a1".repeat(32)));
    assert_eq!(
        f.get_vault(s(&v, "vaultB").parse().unwrap())
            .unwrap()
            .available_notes,
        1
    );
}
#[test]
#[should_panic(expected = "Unregistered vault")]
fn arbitrary_account_cannot_invent_backing_for_a_note() {
    let v = vector();
    let mut f = init(&v);
    context("fake.test.near", REGISTRY_STORAGE, 1);
    f.register_deposit(s(&v, "a"), "a1".repeat(32));
}
#[test]
fn shared_transfer_replaces_note_without_changing_distributed_backing() {
    let v = vector();
    let mut f = funded(&v);
    let (p, inputs) = proof(&v, "transferA");
    context("relay.test.near", STORAGE, 3);
    f.transfer(s(&v, "output"), s(&v, "transferRoot"), p, inputs);
    assert_eq!(f.get_config().root, s(&v, "nextRoot"));
    assert_eq!(f.outstanding, 2);
    let nf = withdraw(&mut f, &v, "outputFromB", "vaultB");
    context("forest.test.near", 0, 6);
    assert!(f.on_dispatched(nf, 1, Ok(true)));
    assert_eq!(f.outstanding, 1);
}
#[test]
fn full_global_tree_refuses_new_backing_without_crediting_liquidity() {
    let v = vector();
    let mut f = init(&v);
    for i in 0..CAPACITY {
        f.leaves.push(&parse(&(i + 1).to_string()));
    }
    context(&s(&v, "vaultA"), REGISTRY_STORAGE, 1);
    assert!(!f.register_deposit(s(&v, "a"), "a1".repeat(32)));
    assert_eq!(f.outstanding, 0);
    assert_eq!(
        f.get_vault(s(&v, "vaultA").parse().unwrap())
            .unwrap()
            .available_notes,
        0
    );
}
