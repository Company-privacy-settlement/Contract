use super::*;
use near_sdk::{
    mock::MockAction,
    test_utils::{get_created_receipts, VMContextBuilder},
    testing_env,
};
const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
fn context(caller: &str, deposit: u128, height: u64) {
    let mut b = VMContextBuilder::new();
    b.current_account_id("a.forest.test.near".parse().unwrap())
        .predecessor_account_id(caller.parse().unwrap())
        .signer_account_id(caller.parse().unwrap())
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(1_000))
        .prepaid_gas(Gas::from_tgas(300))
        .block_height(height);
    testing_env!(b.build());
}
fn init() -> Vault {
    context("forest.test.near", NEAR, 0);
    Vault::new(
        "forest.test.near".parse().unwrap(),
        "sandbox".into(),
        U128(NEAR),
        "treasury.test.near".parse().unwrap(),
    )
}
fn deposit() -> Vault {
    let mut v = init();
    context(
        "alice.test.near",
        NEAR + fees::deposit_fee(NEAR) + STORAGE,
        1,
    );
    v.deposit("123".into(), "ab".repeat(32)).detach();
    v
}
fn active() -> Vault {
    let mut v = deposit();
    context("a.forest.test.near", 0, 2);
    assert!(v.on_registered("123".into(), 1, Ok(true)));
    v
}
fn payout(v: &mut Vault) -> PromiseOrValue<bool> {
    context("forest.test.near", LOCAL_STORAGE, 3);
    v.payout(
        "456".into(),
        "receiver.test.near".parse().unwrap(),
        "cd".repeat(32),
    )
}
#[test]
fn principal_stays_in_vault_while_only_storage_is_forwarded_for_registration() {
    let v = deposit();
    assert_eq!(v.pending, 1);
    assert_eq!(v.available, 0);
    let receipts = get_created_receipts();
    assert!(receipts.iter().any(|r| r.receiver_id.as_str() == "forest.test.near" && r.actions.iter().any(|a|
        matches!(a, MockAction::FunctionCallWeight { attached_deposit, .. } if attached_deposit.as_yoctonear() == REGISTRY_STORAGE))));
    assert!(!receipts
        .iter()
        .flat_map(|r| &r.actions)
        .any(|a| matches!(a, MockAction::Transfer { .. })));
}
#[test]
fn registration_is_idempotent_and_failed_attempts_cannot_override_newer_result() {
    let mut v = deposit();
    context("a.forest.test.near", 0, 2);
    assert!(!v.on_registered("123".into(), 1, Err(PromiseError::Failed)));
    context("anyone.test.near", STORAGE, 3);
    v.retry_deposit("123".into()).detach();
    context("a.forest.test.near", 0, 4);
    assert!(!v.on_registered("123".into(), 1, Ok(false)));
    assert!(v.on_registered("123".into(), 2, Ok(true)));
    assert!(v.on_registered("123".into(), 2, Ok(true)));
    assert_eq!(v.pending, 0);
    assert_eq!(v.available, 1);
    assert_eq!(v.earned_fees, fees::deposit_fee(NEAR));
}
#[test]
fn rejected_registration_refunds_only_original_payer_and_restores_transfer_failure() {
    let mut v = deposit();
    context("a.forest.test.near", 0, 2);
    assert!(!v.on_registered("123".into(), 1, Ok(false)));
    assert_eq!(v.refund_liability, NEAR + fees::deposit_fee(NEAR));
    assert_eq!(v.earned_fees, 0);
    context("anyone.test.near", 1, 3);
    v.claim_deposit_refund("123".into()).detach();
    assert!(get_created_receipts().iter().any(|r| r.receiver_id.as_str() == "alice.test.near" && r.actions.iter().any(|a|
        matches!(a, MockAction::Transfer { deposit, .. } if deposit.as_yoctonear() == NEAR + fees::deposit_fee(NEAR)))));
    context("a.forest.test.near", 0, 4);
    assert!(!v.on_refund("123".into(), Err(PromiseError::Failed)));
    context("anyone.test.near", 1, 5);
    v.claim_deposit_refund("123".into()).detach();
    context("a.forest.test.near", 0, 6);
    assert!(v.on_refund("123".into(), Ok(())));
    assert_eq!(v.held, 0);
    assert_eq!(v.refund_liability, 0);
}
#[test]
#[should_panic(expected = "Only registry may authorize payouts")]
fn external_relayer_cannot_bypass_the_global_nullifier_registry() {
    let mut v = active();
    context("attacker.test.near", LOCAL_STORAGE, 3);
    v.payout(
        "456".into(),
        "receiver.test.near".parse().unwrap(),
        "cd".repeat(32),
    );
}
#[test]
fn pending_and_completed_payout_retries_never_send_principal_twice() {
    let mut v = active();
    match payout(&mut v) {
        PromiseOrValue::Promise(p) => p.detach(),
        _ => panic!("Expected transfer"),
    }
    assert_eq!(v.available, 0);
    assert_eq!(v.reserved, 1);
    assert!(matches!(payout(&mut v), PromiseOrValue::Value(false)));
    assert!(get_created_receipts().is_empty());
    context("a.forest.test.near", 0, 4);
    assert!(v.on_payout("456".into(), Ok(())));
    assert_eq!(v.held, 0);
    assert_eq!(v.reserved, 0);
    assert!(matches!(payout(&mut v), PromiseOrValue::Value(true)));
    assert!(get_created_receipts().is_empty());
}
#[test]
fn failed_native_transfer_remains_reserved_until_same_target_retry_succeeds() {
    let mut v = active();
    match payout(&mut v) {
        PromiseOrValue::Promise(p) => p.detach(),
        _ => panic!("Expected transfer"),
    }
    context("a.forest.test.near", 0, 4);
    assert!(!v.on_payout("456".into(), Err(PromiseError::Failed)));
    assert_eq!(v.reserved, 1);
    assert_eq!(v.available, 0);
    match payout(&mut v) {
        PromiseOrValue::Promise(p) => p.detach(),
        _ => panic!("Expected retry"),
    }
    context("a.forest.test.near", 0, 5);
    assert!(v.on_payout("456".into(), Ok(())));
    assert_eq!(v.reserved, 0);
    assert_eq!(v.held, 0);
    assert_eq!(v.earned_fees, fees::deposit_fee(NEAR));
}
#[test]
#[should_panic(expected = "Deposit is not refundable")]
fn globally_accepted_note_can_never_be_refunded_to_its_depositor() {
    let mut v = active();
    context("alice.test.near", 1, 3);
    v.claim_deposit_refund("123".into()).detach();
}
