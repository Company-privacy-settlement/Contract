# Native USDC / NEP-141 shared settlement — v3

[Service overview](https://github.com/Company-privacy-settlement/Core/blob/main/docs/SERVICE.md) · [Contract build and validation](../README.md)

The shared forest and vault binaries support **either native NEAR v2 or one immutable NEP-141 token v3 per forest**. Initialize v3 with `new_ft`; native `new` retains the v2 wire protocol. A forest never mixes assets or denominations. This implementation remains sandbox/testnet development code; only sandbox/testnet configuration labels are accepted, and no public-network deployment is performed. A label does not detect the underlying chain; it is not a deployment-network security boundary.

## Token identity and units

Circle native USDC on NEAR uses 6 decimal places: 1 USDC = 1,000,000 atomic units. Pin the token account in both `new_ft` and the client policy. A token symbol or user-supplied metadata is not an allowlist.

| Network | Circle native USDC token account |
| --- | --- |
| Testnet | `3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af` |
| Mainnet, reference only | `17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1` |

Sources checked 2026-11-12: [Circle contract addresses](https://developers.circle.com/stablecoins/usdc-contract-addresses), [NEAR FT integration](https://docs.near.org/integrations/fungible-tokens), [NEP-141](https://github.com/near/NEPs/blob/master/neps/nep-0141.md). Bridged USDC.e is a different asset and must not substitute for these accounts.

USDC balances live in the issuer's NEP-141 ledger under each vault account. Settlement principal is distributed among those vault balances. Only withdrawal protocol fees enter the forest's token balance. Circle retains the token's issuer controls, including possible freezing; this protocol cannot remove those controls.

## Domain and proof

The same five-input Groth16 circuit is used, with a new asset-bound domain and action prefix:

```text
domain = sha253("core:ft:v3|" + network + "|" + registry + "|" + token_id + "|" + denomination)
contextHash = sha253("core:action:ft:v3|" + domain + "|" + kind + "|" + vault + "|" + target + "|" + receiptRoot)
```

For transfers, vault is empty and target is the new note commitment. For withdrawals, vault and recipient are mandatory destination bindings. Native v1/v2 proofs, other token domains and different payout destinations are rejected. Principal remains in separate vaults, while membership, historic roots and spent nullifiers are shared within this one FT group. The registry creates keyless children and passes the immutable token account into each `new_ft` call.

## Fees: never add different currencies

For a 1 USDC denomination:

| Operation | Tokens sent | Recipient/private principal | NEAR costs |
| --- | --- | --- | --- |
| Deposit | **1.0015 USDC** to selected vault | **1 USDC** note | 0.02 NEAR action storage |
| Withdraw | **0.0015 USDC** fee to forest | **1 USDC** paid from chosen vault | 0.02 NEAR action storage |
| Private note transfer | None | New 1 USDC note | 0.02 NEAR storage |
| Retry deposit/withdrawal | No second token fee | Same bound operation | 0.02 NEAR storage |

Fees round down in token atomic units. FT denominations whose 15 bps fee rounds to zero are rejected. For USDC the minimum permitted denomination is 667 atomic units; deployments typically choose larger fixed denominations.

Gas is paid in NEAR. `ft_transfer_call`, outgoing `ft_transfer`, and refund/collection authorization each use exactly 1 yoctoNEAR as applicable. FT account registration is additional and depends on the token's `storage_balance_bounds`.

Because `ft_on_transfer` receives no attached NEAR, each token sender prepays action storage on the destination vault (deposits) or forest (withdrawal fees):

```text
prepay_storage(account_id = token_sender) with 0.025 NEAR on first use
  0.005 NEAR permanent credit-entry storage bond
  0.020 NEAR spendable action credit
```

Later top-ups charge no new bond. Third parties may sponsor a named sender's credit; only that beneficiary can withdraw its unused credit. `withdraw_storage_credit` with 1 yoctoNEAR refunds available credit to its owner, and a failed transfer restores the credit. The storage bond remains to fund the persistent ledger entry. In-flight credits and token liabilities are kept in separate ledgers; token principal/fees are never treated as NEAR reserves.

## Deposit

1. Deploy a new forest using `new_ft(network, denomination, verification_key, fee_recipient, token_id)`, create vaults and independently verify code hashes and removed keys.
2. Register the vault on the pinned token with NEP-145 `storage_deposit`. Fund NEAR storage credit for the **token sender**, not the signer reported by an untrusted receiver.
3. Call the pinned token's `ft_transfer_call` with receiver = selected vault, amount = denomination + 15 bps, attached deposit = 1 yoctoNEAR, 300 Tgas, and message:

```json
{"commitment":"CANONICAL_FIELD","disclosure_root":"64_LOWERCASE_HEX"}
```

The vault checks that the predecessor is its immutable token contract, validates the exact amount and message, consumes that sender's storage credit, and synchronously returns `"0"` (all tokens accepted). Registration runs in a **detached promise**, with the existing idempotent registration/retry state machine. Returning the registration promise to the FT resolver would risk refunding backing after a lost callback despite a minted note, so it is deliberately avoided.

Wait for `get_deposit(...).state == Active`. A successful `ft_transfer_call` only confirms token acceptance, not registration completion. Same-vault duplicates return all incoming tokens immediately without consuming more storage. Explicit forest rejection makes the accepted principal + token fee refundable to the original `sender_id` via `claim_deposit_refund`. A registry error remains retryable and never establishes safe refundability. Failed token refunds restore the refund liability.

## Withdrawal

Register the forest (fee receiver), chosen vault, treasury and payout recipient on the pinned token. An implicit NEAR account must also exist before token registration; generating a new public key alone does not register it on USDC.

Prepay forest storage credit for the fee payer. Call the token's `ft_transfer_call`, receiver = forest, amount = exactly the 15 bps token fee, attached = 1 yoctoNEAR, 300 Tgas:

```json
{"vault":"VERIFIED_VAULT","recipient":"REGISTERED_RECIPIENT","disclosure_root":"64_LOWERCASE_HEX","proof":"BASE64_PROOF","public_inputs":["nullifier","authorization","root","contextHash","domain"]}
```

The forest authenticates the token caller, checks the exact fee and proof, consumes the global nullifier, reserves vault liquidity and escrows the token fee. It synchronously accepts the fee and dispatches payout separately. The vault sends the **full denomination** via `ft_transfer` with 1 yoctoNEAR and confirms the result. Only confirmed success earns the withdrawal fee.

Wrong token, wrong fee, invalid proof, replay or missing storage makes the receiver call fail atomically and the standard token resolver refunds the fee. After acceptance, a failed payout (including an unregistered/frozen recipient) permanently retains the spent nullifier and original destination. `retry_withdrawal(nullifier)` attaches NEAR storage only and charges no second token fee. Observe `get_withdrawal(...).state == Complete`; token acceptance and requested events are not payout evidence. Permanently missing vault transfer callbacks retain the same conservative pending limitation described in [SHARED.md](SHARED.md).

Fee collections send only earned tokens to the fixed treasury and restore the earned ledger on failure. No principal sweep, token setter, denomination setter or nullifier reset is added.

## Events and SDK

Forest settlement records use `core_shared` version **3.0.0** and include `asset_token_id`. Amount and protocol fee fields are token atomic units; `storage_fee_charged` remains yoctoNEAR. Evidence bundles use `version: 3` and `assetTokenId`; verification anchors the token identity to finalized executor logs and never labels FT amounts as NEAR. V2 evidence remains native NEAR.

The Core SDK provides `Pool { version: 3, tokenId, ... }`, token-aware discovery, exact USDC parsing, separate-currency fee quotes, explicit FT registration/storage prepayment call builders and FT deposit/withdrawal builders. CLI `shared-*` commands support v3 policies. Contract call plans still require an actual NEAR wallet to sign them.

## Reproducible sandbox validation

```sh
# Contract checkout
npm run build
npm run build:test-ft
npm test
# Core checkout, with a sibling Contract checkout or CORE_CONTRACT_REPO
npm run contract:build
# Existing proving artifacts must match committed vectors; a new setup needs new vectors.
npm run demo:ft
```

`contracts/test-ft` embeds the official near-contract-standards implementation plus owner-controlled failure injection. It is **not Circle USDC** and is excluded from normal production build manifests. The demo deploys it only to a disposable sandbox. It exercises token refunds, counterfeit rejection, cross-vault membership, a global double-spend race, unregistered/frozen recipients, payout/refund/fee-collection recovery and finalized asset-aware evidence. `demo-output/ft-validation.json` is a local-chain report, not a public-network transaction record.

The new binary adds contract state fields. It must be deployed as a **new group**, not installed over old Borsh state. Existing keyless deployments cannot be upgraded in place; there is no migration or private cross-asset bridge in this change. Development trusted setup and unaudited production limitations remain.
