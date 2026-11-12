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
    let mut f = if v["pool"]["version"] == 3 {
        Forest::new_ft(
            "sandbox".into(),
            U128(1_000_000),
            serde_json::from_value(v["verificationKey"].clone()).unwrap(),
            "treasury.test.near".parse().unwrap(),
            "usdc.test.near".parse().unwrap(),
        )
    } else {
        Forest::new(
            "sandbox".into(),
            U128(NEAR),
            serde_json::from_value(v["verificationKey"].clone()).unwrap(),
            "treasury.test.near".parse().unwrap(),
        )
    };
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
fn ft_vector() -> Value {
    serde_json::from_str(include_str!("../../../test-vectors/shared-ft.json")).unwrap()
}
fn ft_funded() -> (Forest, Value) {
    let v = ft_vector();
    let f = funded(&v);
    (f, v)
}
fn ft_withdraw(f: &mut Forest, v: &Value, key: &str, vault: &str) -> String {
    context("relay.test.near", STORAGE + ft::STORAGE_CREDIT_BOND, 4);
    f.prepay_storage(None);
    let (p, inputs) = proof(v, key);
    let nf = inputs[0].clone();
    context("usdc.test.near", 0, 5);
    assert_eq!(f.ft_on_transfer("relay.test.near".parse().unwrap(), U128(1500), serde_json::json!({"vault": s(v,vault),"recipient":s(v,"recipient"),"disclosure_root":s(v,"disclosureRoot"),"proof":p,"public_inputs":inputs}).to_string()).0, 0);
    nf
}
#[test]
fn ft_group_domain_binds_token_and_keeps_currency_units_separate() {
    let (mut f, v) = ft_funded();
    assert_eq!(f.get_config().protocol, "core-shared-ft-registry-v3");
    assert_eq!(f.get_config().domain, s(&v, "domain"));
    assert_ne!(s(&v, "domain"), s(&vector(), "domain"));
    assert_eq!(f.get_config().deposit_attached.0, STORAGE);
    assert_eq!(f.get_config().deposit_token_amount.unwrap().0, 1_001_500);
    let nf = ft_withdraw(&mut f, &v, "aFromB", "vaultB");
    assert!(f.is_spent(nf.clone()));
    assert_eq!(f.pending_fees, 1500);
    assert_eq!(f.storage_credits.liability, 0);
    assert_eq!(
        f.get_vault(s(&v, "vaultB").parse().unwrap())
            .unwrap()
            .reserved_notes,
        1
    );
    context("forest.test.near", 0, 6);
    assert!(f.on_dispatched(nf, 1, Ok(true)));
    assert_eq!(f.earned_fees, 1500);
    assert_eq!(f.outstanding, 1);
    let r = f.records.get(2).unwrap();
    assert_eq!(r.asset_token_id.unwrap().as_str(), "usdc.test.near");
    assert_eq!(r.protocol_fee_charged.0, 1500);
    assert_eq!(r.storage_fee_charged.0, STORAGE);
}
#[test]
fn ft_failed_dispatch_keeps_fee_and_nullifier_and_retry_charges_no_second_token_fee() {
    let (mut f, v) = ft_funded();
    let nf = ft_withdraw(&mut f, &v, "aFromB", "vaultB");
    context("forest.test.near", 0, 6);
    assert!(!f.on_dispatched(nf.clone(), 1, Ok(false)));
    assert!(f.is_spent(nf.clone()));
    assert_eq!(f.pending_fees, 1500);
    context("anyone.test.near", STORAGE, 7);
    f.retry_withdrawal(nf.clone()).detach();
    context("forest.test.near", 0, 8);
    assert!(!f.on_dispatched(nf.clone(), 1, Ok(true)));
    assert!(f.on_dispatched(nf, 2, Ok(true)));
    assert_eq!(f.pending_fees, 0);
    assert_eq!(f.earned_fees, 1500);
    assert_eq!(f.outstanding, 1);
}
#[test]
#[should_panic(expected = "Untrusted FT contract")]
fn fake_ft_cannot_pay_a_withdrawal_fee() {
    let (mut f, _) = ft_funded();
    context("fake.test.near", 0, 4);
    f.ft_on_transfer("relay.test.near".parse().unwrap(), U128(1500), "{}".into());
}
#[test]
#[should_panic(expected = "exact add-on withdrawal token fee")]
fn ft_withdrawal_cannot_omit_the_add_on_fee() {
    let (mut f, _) = ft_funded();
    context("usdc.test.near", 0, 4);
    f.ft_on_transfer("relay.test.near".parse().unwrap(), U128(1499), "{}".into());
}
#[test]
#[should_panic(expected = "Wrong shared domain")]
fn native_proof_cannot_spend_ft_principal() {
    let (mut f, _) = ft_funded();
    ft_withdraw(&mut f, &vector(), "aFromB", "vaultB");
}
#[test]
#[should_panic(expected = "Wrong shared domain")]
fn other_token_domain_cannot_spend_ft_principal() {
    let (mut f, v) = ft_funded();
    f.domain = sha_field("different token domain");
    ft_withdraw(&mut f, &v, "aFromB", "vaultB");
}
#[test]
#[should_panic(expected = "Global nullifier already spent")]
fn ft_note_cannot_pay_from_two_different_vaults() {
    let (mut f, v) = ft_funded();
    ft_withdraw(&mut f, &v, "aFromB", "vaultB");
    // The existing payer credit entry needs no second bond.
    let (p, inputs) = proof(&v, "aFromA");
    context("relay.test.near", STORAGE, 6);
    f.prepay_storage(None);
    context("usdc.test.near", 0, 7);
    f.ft_on_transfer("relay.test.near".parse().unwrap(),U128(1500),serde_json::json!({"vault":s(&v,"vaultA"),"recipient":s(&v,"recipient"),"disclosure_root":s(&v,"disclosureRoot"),"proof":p,"public_inputs":inputs}).to_string());
}
#[test]
fn ft_transfer_updates_shared_tree_without_moving_any_tokens() {
    let (mut f, v) = ft_funded();
    let (p, inputs) = proof(&v, "transferA");
    context("relay.test.near", STORAGE, 3);
    assert_eq!(
        f.transfer(s(&v, "output"), s(&v, "transferRoot"), p, inputs),
        2
    );
    assert_eq!(f.get_config().root, s(&v, "nextRoot"));
    assert_eq!(f.outstanding, 2);
    assert!(get_created_receipts().is_empty());
}
