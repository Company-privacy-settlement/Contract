# Shared anonymity with distributed custody — v2

[Service overview](https://github.com/Company-privacy-settlement/Core/blob/main/docs/SERVICE.md) · [Contract build and validation](../README.md)

This page describes native NEAR v2. For token-bound USDC/NEP-141 v3, see [USDC.md](USDC.md).

## Trust and funds

One immutable forest defines one group `(network label, registry account, denomination, verification key, vault bytecode, fee recipient)`. It creates keyless child vaults, stores a shared append-only Poseidon tree and global spent-nullifier set, and serializes authorization on NEAR. It never receives settlement principal. Vaults retain the native NEAR from deposits and pay recipients directly.

The registry is a shared on-chain coordination point, not an operator-controlled approval service. Its keys must be removed after deployment; clients verify empty key lists and pinned bytecode for both registry and vaults. It temporarily handles vault bootstrap funds/refunds and withdrawal protocol fees, which are separate from principal. There is no arbitrary vault-call, principal sweep, verifier setter, redirect, or nullifier-reset method.

## Proof statement

The existing five-public-input Groth16 membership circuit is reused. Public order remains `nullifierHash, authorization, root, contextHash, domain`. The note format uses `Poseidon(secret, nullifier, domain)` and does not encode an origin vault.

```text
domain = sha253("core:v2|" + network + "|" + registry + "|" + denomination)
contextHash = sha253("core:action:v2|" + domain + "|" + kind + "|" + vault + "|" + target + "|" + receiptRoot)
```

`vault` is empty for a private transfer, whose target is the new note commitment. For withdrawal it is the chosen payout vault and target is the public recipient. A proof cannot be redirected to another vault or recipient. The global nullifier is consumed before cross-contract dispatch. Two valid proofs for one secret and different destinations compete for the same global nullifier; one can succeed.

All commitments, including transferred outputs, enter the shared tree. Historical roots remain valid. A transfer changes the note owner but does not move funds between vaults or change backing. Withdrawal may use any verified vault with available liquidity, regardless of the deposited note's origin. Global capacity is 256 outputs, not 256 per vault. Full groups can still withdraw; rejected fresh deposits are refundable.

## Deposit state machine

1. User calls `vault.deposit(commitment, disclosure_root)` with principal + 0.15% + total storage fee.
2. Vault records pending principal/fee and forwards only the registry's storage share.
3. Forest accepts deposits only from `Ready` vaults it deployed. First registration appends the commitment and credits liquidity once. Identical registration retries return the same success without creating another leaf or another credit. Another vault reusing the commitment, or a full tree, returns rejection.
4. Vault callback activates local liquidity and earns the deposit fee on acceptance. Explicit rejection makes principal + protocol fee refundable to the original payer. A failed registry call stays retryable; it is not assumed accepted or refunded.

`retry_deposit(commitment)` takes storage only. Pending callbacks can be resumed after 20 blocks; failed calls can retry immediately. Attempt counters prevent old callbacks from overwriting a newer state. An accepted commitment can never be refunded merely because a response was delayed. Refund destinations are permanently bound to the original depositor, and failed refund transfers restore refundability. Storage/gas costs of failed attempts are not reimbursed.

## Withdrawal state machine

1. Call `forest.withdraw(vault, recipient, disclosure_root, proof, public_inputs)` with 0.15% + storage, while the note principal stays in vaults.
2. Forest verifies group domain, historical root, complete destination binding, proof and unused nullifier. It consumes the nullifier, reserves one note of the chosen vault's liquidity, and escrows the withdrawal fee.
3. Selected vault accepts only its immutable registry caller. It records the nullifier before scheduling a native transfer. Duplicate pending requests return false; already completed requests return true without another transfer.
4. A vault transfer callback records `Complete` or `Retryable`. The forest callback releases the reservation and earns the fee only after confirmed success.

`forest.retry_withdrawal(nullifier)` takes storage only and always uses the original vault and recipient. It does not release the nullifier, charge a new protocol fee, reserve a second note or redirect funds. Failed requests retry immediately; unacknowledged registry requests can resume after 20 blocks. Attempt IDs ignore stale forest callbacks. A completed vault payment whose registry acknowledgment was lost can be acknowledged by an idempotent retry.

Timeouts never establish that a payment failed. A vault whose own native-transfer callback remains permanently `Pending` cannot be safely reset or redirected by this version; it is kept reserved rather than risking a second payment. Fixed bounded callback gas is supplied and exercised in sandbox tests. Native-transfer failure and lost registry acknowledgment recovery are tested. A production system would require further audited recovery guarantees.

## APIs

Forest: `new(network, denomination, verification_key, fee_recipient)`, `get_config`, `predict_vault`, `create_vault`, `list_vaults`, `get_vault`, bootstrap refund methods, `register_deposit` (registered vaults only), `get_commitments`, `get_records`, `transfer`, `withdraw`, `retry_withdrawal`, `get_withdrawal`, `is_spent`, `collect_protocol_fees`.

Vault: `new(registry, network, denomination, fee_recipient)` (direct registry parent only), `get_config`, `deposit`, `retry_deposit`, `get_deposit`, `claim_deposit_refund`, `payout` (registry only), `get_payout`, `collect_protocol_fees`.

Use 300 Tgas for user writes. The SDK reads one finalized block, verifies registry/vault code hashes, keys, group/fee parameters and liquidity, rebuilds the shared tree, and binds the chosen payout vault into the proof. A stale liquidity quote can cause a new withdrawal to reject before nullifier consumption; the client can then choose another funded vault.

## Fees, solvency and evidence

Protocol rates remain 15 bps on top of principal, rounded down in yoctoNEAR. Total storage remains 0.02 NEAR; registry keeps 0.015 and vault receives 0.005. A private transfer pays 0.02 directly to the registry. The vault earns accepted deposit fees; forest earns successful withdrawal fees. Pending fees and principal/refund liabilities are excluded from treasury collection. Failed fee collections restore the earned-fee ledger.

Forest journal events use `standard: core_shared`, `version: 2.0.0`, and identify the public payout/registration vault. They include principal, charged fee, earned fee and storage charge separately. Retried payout records carry zero new protocol fee. The SDK's v2 evidence bundle verifies finalized registry receipts and journal entries without assuming that all transactions were disclosed or that input/output notes are publicly linked.

## Privacy boundaries

Principal does not follow an A → B → C route. Private transfers update shared commitments without moving vault backing; one chosen funded vault pays the bound recipient directly. Deploying more vaults distributes custody but does not itself increase the number of independent participants or guarantee an effective anonymity set. The shared forest still coordinates membership and global nullifiers.

Groth16 does not name the spent input commitment in a withdrawal. Public observers still see commitments, funding, vaults, signers, recipients, amounts, timing, nullifiers and disclosure references. Address reuse and small groups can correlate payments. New Core invoice terms v2 derive a separate signed reference per payment unit rather than reuse one common invoice root. This requires no ABI, circuit or note-domain change. Invoice references, deadlines and unit caps are not on-chain reservations: distinct nullifiers may pay the same reference, and clients must report actual payments honestly. Existing v1 invoice roots remain valid.

See [Core’s privacy model](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PRIVACY.md) for the API/link-holder boundary, selected evidence and invoice reconciliation limits.

## Limits

Shared cryptographic membership does not guarantee network anonymity. Timing, small participant counts, public recipients, fixed amounts, funding patterns, relayer identities and disclosed receipt fields can reduce privacy. Groups and v1 pools have separate domains and are not private bridges to each other. Network labels are configuration, not automatic detection of the underlying chain. This is a single-machine trusted setup for local/testnet development, not an audited production deployment.
