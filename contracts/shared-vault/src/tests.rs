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

const USD: u128 = 1_000_000;
fn ft_init() -> Vault {
    context("forest.test.near", NEAR, 0);
    Vault::new_ft(
        "forest.test.near".parse().unwrap(),
        "sandbox".into(),
        U128(USD),
        "treasury.test.near".parse().unwrap(),
        "usdc.test.near".parse().unwrap(),
    )
}
fn ft_deposit() -> Vault {
    let mut v = ft_init();
    context("alice.test.near", STORAGE + ft::STORAGE_CREDIT_BOND, 1);
    v.prepay_storage(None);
    context("usdc.test.near", 0, 2);
    assert_eq!(
        v.ft_on_transfer(
            "alice.test.near".parse().unwrap(),
            U128(USD + 1500),
            near_sdk::serde_json::json!({"commitment": "123", "disclosure_root": "ab".repeat(32)})
                .to_string()
        )
        .0,
        0
    );
    v
}
#[test]
fn ft_accepts_exact_principal_plus_fee_and_consumes_near_storage_separately() {
    let v = ft_deposit();
    assert_eq!(v.pending, 1);
    assert_eq!(v.pending_fees, 1500);
    assert_eq!(v.storage_credits.liability, 0);
    let config = v.get_config();
    assert_eq!(config["deposit_token_amount"], "1001500");
    assert_eq!(config["deposit_attached"], STORAGE.to_string());
    assert_eq!(
        v.get_deposit("123".into()).unwrap().owner.as_str(),
        "alice.test.near"
    );
    assert!(get_created_receipts()
        .iter()
        .any(|r| r.receiver_id.as_str() == "forest.test.near"));
}
#[test]
#[should_panic(expected = "Untrusted FT contract")]
fn fake_token_cannot_create_ft_backing() {
    let mut v = ft_init();
    context("fake.test.near", 0, 1);
    v.ft_on_transfer(
        "alice.test.near".parse().unwrap(),
        U128(USD + 1500),
        "{}".into(),
    );
}
#[test]
#[should_panic(expected = "add-on token fee")]
fn ft_principal_without_add_on_fee_is_rejected() {
    let mut v = ft_init();
    context("usdc.test.near", 0, 1);
    v.ft_on_transfer(
        "alice.test.near".parse().unwrap(),
        U128(USD),
        near_sdk::serde_json::json!({"commitment":"123","disclosure_root":"ab".repeat(32)})
            .to_string(),
    );
}
#[test]
#[should_panic(expected = "Prepay NEAR storage")]
fn token_deposit_cannot_spend_another_payers_storage_credit() {
    let mut v = ft_init();
    context("alice.test.near", STORAGE + ft::STORAGE_CREDIT_BOND, 1);
    v.prepay_storage(None);
    context("usdc.test.near", 0, 2);
    v.ft_on_transfer(
        "attacker.test.near".parse().unwrap(),
        U128(USD + 1500),
        near_sdk::serde_json::json!({"commitment":"123","disclosure_root":"ab".repeat(32)})
            .to_string(),
    );
}
#[test]
fn same_vault_duplicate_returns_all_tokens_without_charging_more_storage() {
    let mut v = ft_deposit();
    context("usdc.test.near", 0, 3);
    let unused = v.ft_on_transfer(
        "alice.test.near".parse().unwrap(),
        U128(USD + 1500),
        near_sdk::serde_json::json!({"commitment":"123","disclosure_root":"ab".repeat(32)})
            .to_string(),
    );
    assert_eq!(unused.0, USD + 1500);
    assert_eq!(v.held, 1);
    assert_eq!(v.pending_fees, 1500);
    assert!(get_created_receipts().is_empty());
}
#[test]
fn rejected_ft_registration_refunds_principal_and_fee_to_original_token_sender() {
    let mut v = ft_deposit();
    context("a.forest.test.near", 0, 3);
    assert!(!v.on_registered("123".into(), 1, Ok(false)));
    context("relay.test.near", 1, 4);
    v.claim_deposit_refund("123".into()).detach();
    assert!(ft_transfer_to("alice.test.near", USD + 1500));
    context("a.forest.test.near", 0, 5);
    assert!(!v.on_refund("123".into(), Err(PromiseError::Failed)));
    assert_eq!(v.refund_liability, USD + 1500);
    assert_eq!(v.held, 1);
    context("relay.test.near", 1, 6);
    v.claim_deposit_refund("123".into()).detach();
    context("a.forest.test.near", 0, 7);
    assert!(v.on_refund("123".into(), Ok(())));
    assert_eq!(v.held, 0);
    assert_eq!(v.refund_liability, 0);
}
fn ft_transfer_to(recipient: &str, amount: u128) -> bool {
    get_created_receipts().iter().any(|r| {
        r.receiver_id.as_str() == "usdc.test.near"
            && r.actions.iter().any(|action| {
                if let MockAction::FunctionCallWeight {
                    method_name,
                    args,
                    attached_deposit,
                    ..
                } = action
                {
                    let value: near_sdk::serde_json::Value =
                        near_sdk::serde_json::from_slice(args).unwrap();
                    method_name == b"ft_transfer"
                        && attached_deposit.as_yoctonear() == 1
                        && value["receiver_id"] == recipient
                        && value["amount"] == amount.to_string()
                } else {
                    false
                }
            })
    })
}
#[test]
fn ft_payout_retries_full_principal_once_and_collects_only_earned_token_fees() {
    let mut v = ft_deposit();
    context("a.forest.test.near", 0, 3);
    assert!(v.on_registered("123".into(), 1, Ok(true)));
    if let PromiseOrValue::Promise(p) = payout(&mut v) {
        p.detach();
    } else {
        panic!();
    }
    assert!(ft_transfer_to("receiver.test.near", USD));
    context("a.forest.test.near", 0, 4);
    assert!(!v.on_payout("456".into(), Err(PromiseError::Failed)));
    assert_eq!(v.reserved, 1);
    if let PromiseOrValue::Promise(p) = payout(&mut v) {
        p.detach();
    } else {
        panic!();
    }
    context("a.forest.test.near", 0, 5);
    assert!(v.on_payout("456".into(), Ok(())));
    let logs = near_sdk::test_utils::get_logs();
    assert!(logs.iter().any(|l| l.contains("\"version\":\"3.0.0\"")
        && l.contains("\"asset_token_id\":\"usdc.test.near\"")));
    assert!(matches!(payout(&mut v), PromiseOrValue::Value(true)));
    assert!(get_created_receipts().is_empty());
    context("anyone.test.near", 1, 6);
    v.collect_protocol_fees().detach();
    assert!(ft_transfer_to("treasury.test.near", 1500));
    context("a.forest.test.near", 0, 7);
    assert!(!v.on_fees(U128(1500), Err(PromiseError::Failed)));
    assert_eq!(v.earned_fees, 1500);
}
#[test]
fn prepaid_near_refunds_are_owner_bound_and_failed_refunds_restore_credit() {
    let mut v = ft_init();
    context("alice.test.near", 2 * STORAGE + ft::STORAGE_CREDIT_BOND, 1);
    v.prepay_storage(None);
    context("alice.test.near", 1, 2);
    v.withdraw_storage_credit().detach();
    assert!(get_created_receipts().iter().any(|r| r.receiver_id.as_str() == "alice.test.near" && r.actions.iter().any(|a| matches!(a, MockAction::Transfer { deposit, .. } if deposit.as_yoctonear() == 2 * STORAGE))));
    assert_eq!(v.storage_credits.liability, 2 * STORAGE);
    context("a.forest.test.near", 0, 3);
    assert!(!v.on_storage_refund(
        "alice.test.near".parse().unwrap(),
        U128(2 * STORAGE),
        Err(PromiseError::Failed)
    ));
    assert_eq!(
        v.get_storage_credit("alice.test.near".parse().unwrap())
            .unwrap()
            .available
            .0,
        2 * STORAGE
    );
    context("alice.test.near", 1, 4);
    v.withdraw_storage_credit().detach();
    context("a.forest.test.near", 0, 5);
    assert!(v.on_storage_refund(
        "alice.test.near".parse().unwrap(),
        U128(2 * STORAGE),
        Ok(())
    ));
    assert_eq!(v.storage_credits.liability, 0);
}
#[test]
fn longest_implicit_token_sender_fits_the_fixed_storage_budget() {
    context("forest.test.near", NEAR, 0);
    let token = "1".repeat(64);
    let owner = "2".repeat(64);
    let mut v = Vault::new_ft(
        "forest.test.near".parse().unwrap(),
        "sandbox".into(),
        U128(USD),
        "treasury.test.near".parse().unwrap(),
        token.parse().unwrap(),
    );
    context("sponsor.test.near", STORAGE + ft::STORAGE_CREDIT_BOND, 1);
    v.prepay_storage(Some(owner.parse().unwrap()));
    context(&token, 0, 2);
    let commitment =
        "21888242871839275222246405745257275088548364400416034343698204186575808495616";
    assert_eq!(v.ft_on_transfer(owner.parse().unwrap(), U128(USD + 1500), near_sdk::serde_json::json!({"commitment":commitment,"disclosure_root":"ab".repeat(32)}).to_string()).0,0);
    assert_eq!(
        v.get_deposit(commitment.into()).unwrap().owner.as_str(),
        owner
    );
}
