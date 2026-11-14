# Contract — Core settlement on NEAR

[English](README.md) · [한국어](README.ko.md) · [简体中文](README.zh-CN.md)

Smart contracts, Groth16 circuit, public vectors and reproducible WASM builds for Core’s private business settlement protocol. [Core](https://github.com/Company-privacy-settlement/Core) provides proving, SDK, read-only API and indexing; [Front](https://github.com/Company-privacy-settlement/Front) provides the English-first landing, Docs, wallet tools, signed invoices and evidence workspace.

[Service overview](https://github.com/Company-privacy-settlement/Core/blob/main/docs/SERVICE.md) · [Development roadmap](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ROADMAP.md) · [Web Docs](https://core-private-settlement-0574c4.jiheon6768.chatgpt.site/docs/)

The separate [Core organization invoice API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_INVOICES.md) now stores immutable signed invoices per organization with idempotency and encryption at rest. This is off-chain service storage; contract ABI, fees and payment proof verification are unchanged.

The [organization payment dashboard](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PAYMENT_DASHBOARD.md) searches/exports signed invoices and derives observed payment states from verified finalized successful withdrawals. Pending/failed wallet attempts do not count. This SDK/Front feature preserves contract ABI, fees and proof permissions.

The separate [signed payment webhook service](https://github.com/Company-privacy-settlement/Core/blob/main/docs/WEBHOOKS.md) consumes verified finalized successful withdrawals with linked transaction provenance. HMAC events, durable retries and receiver deduplication remain service features; contract ABI, principal custody and proof permissions are unchanged.

The [worker/contractor batch workflow](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PAYOUT_BATCHES.md) orchestrates existing single-note withdrawals with independent recipient approvals, additive fee review and fixed-destination recovery. It adds no pooled payroll wallet, atomic multicall ABI or contract-enforced organization custody.

## Architecture and versions

| Component | Responsibility |
| --- | --- |
| `contracts/forest` | Shared note tree, global nullifiers, payout reservations and keyless vault creation |
| `contracts/shared-vault` | Independent principal custody, deposit registration, idempotent payouts and failure refunds |
| `contracts/pool`, `contracts/factory` | Compatible v1 independent pools and factory |
| `circuits/spend.circom` | Poseidon membership, nullifiers and destination/context binding with real Groth16 proofs |

V2 uses native NEAR. V3 pins one NEP-141 token, including native USDC, per group and binds it into the proof domain. V1 remains an independent-pool compatibility path. V1/v2 share the membership circuit but use different domains/action bindings; notes and policies cannot be interchanged arbitrarily.

Principal stays in individual vaults. Shared notes authorize a direct payout from one funded vault, including a vault different from the deposit vault. The forest coordinates proof state and reserves liquidity; it has no principal-sweep authority. There is no sequence of principal hops. Operations require no operator signature, and deployment adds no keys to child contracts. Public recipients, signers, amounts, vaults, references and timing remain visible; more vaults alone do not guarantee anonymity.

Each group has a fixed denomination and depth-8 tree: **256 total outputs**, not 256 reusable slots. Deposits and private transfers consume capacity; withdrawals do not restore it. Groups remain separate proof/privacy domains. [Shared specification](docs/SHARED.md) · [Privacy boundaries](docs/SHARED.md#privacy-boundaries).

## Build and test

Requires Node.js 22+, pinned Rust 1.99.1, the WASM target and C/C++ build tools. Run from this repository.

```sh
npm ci
rustup target add wasm32-unknown-unknown
npm run build
npm test
```

Build order is pool → factory → shared-vault → forest; parent contracts embed optimized child WASM. Outputs in `artifacts/` include `core_pool.wasm`, `core_factory.wasm`, `core_shared_vault.wasm`, `core_forest.wasm` and `build-manifest.json`. Build before contract tests. Core verifies artifact hashes before copying them.

```sh
# Single-machine development setup; a new setup needs a new verifier and deployment.
npm run zk:setup
```

Only public proofs, verification keys and vectors are committed. Secret notes, private keys and generated proving artifacts must remain untracked. This development setup is not a reviewed multi-party ceremony.

## Integrated CI

Pushes and PRs call Core's commit-pinned pipeline with this Contract event SHA and a pinned Core/Front baseline. It checks actual WASM/units, shared vectors, Front SDK freshness, Core/Front/browser suites, native/FT Groth16 sandboxes and an exact-source development release. Private repositories need read-only checkout credentials and Core reusable-workflow access. [Pins, evidence and setup](https://github.com/Company-privacy-settlement/Core/blob/main/docs/CI.md). CI does not deploy or replace an existing verifier.

## End-to-end local chain

Check out `Contract/` and `Core/` as siblings. From Contract:

```sh
cd ../Core
npm ci
npm run contract:build
npm run zk:setup
(cd ../Contract && npm run build:test-ft)
npm run demo:ft
```

Core supports a different contract checkout through `CORE_CONTRACT_REPO`. The FT sandbox uses real WASM and Groth16 proofs with a local **TESTUSD fixture, not Circle USDC**. The latest recorded run on 2026-11-13 produced 14 finalized records and covered fees/recovery, API/index/journal operations, recipient readiness, browser note/receipt backups and finalized evidence anchoring. `npm run demo:shared` in Core exercises native NEAR v2, cross-vault payouts, concurrent double-spend rejection and failed-recipient retries. These disposable-chain demos do not deploy a public network or prove a real-wallet smoke test.

## Assets and additive fees

Deposit and withdrawal each charge **0.15% on top of principal**, rounded down in asset atomic units. The recipient receives full principal.

| Fixed denomination | Deposit | Separate withdrawal payment | Recipient |
| --- | --- | --- | --- |
| 1 USDC | 1.0015 USDC | 0.0015 USDC | 1 USDC |
| 1 NEAR, native v2 | 1.0215 NEAR including storage | 0.0215 NEAR including storage | 1 NEAR |

For FT groups, applicable action storage is 0.02 NEAR and first-use storage credit adds a 0.005 NEAR ledger bond per sender/target contract. Account activation, token registration, gas and FT transfer’s one-yocto attachment are separate. Native v2’s 0.02 NEAR storage splits into 0.015 registry + 0.005 vault; this is not an extra protocol fee. Retries/private transfers pay applicable storage, not another protocol fee. Deposit fees accrue in the vault; confirmed withdrawal fees accrue in the forest. Only earned fees can go to the fixed treasury; principal and pending liabilities are not treasury funds.

`new_ft` creates a separate immutable-token group. Browser settlement currently supports configured testnet FT groups; native NEAR uses Core CLI. [USDC/NEP-141 specification and reference addresses](docs/USDC.md).

## Service integration and limits

Core's separate [Organization API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATIONS.md) manages off-chain team membership and roles. Those roles do not grant contract signing, note ownership, payout redirection or verified corporate identity.

[Organization backup and recovery](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_BACKUPS.md) captures consistent online snapshots, discards sessions/challenges and restores into quarantine. A separately approved current-access file is required before activation into a new database; historical owners, roles and pending invitations are never automatically restored as authority.

[Corporate payment approval](https://github.com/Company-privacy-settlement/Core/blob/main/docs/CORPORATE_PAYMENTS.md) separates the creator from another Owner/Admin approver and freezes the payment, additive 0.15% fee and reviewed prerequisite calls. The supported wallet flow rechecks current membership and approval before reserving each exact call once. This is an organization API/SDK/app policy; the existing contracts retain permissionless fixed-destination proof execution.

Core invoices v2 bind a distinct signed reference per payment unit using the existing ABI/circuit. Reference uniqueness, expiry and payment caps are application checks, not contract-enforced reservations. Concurrent payments and overpayments remain possible. An arbitrary receipt root cannot replace a signed invoice payment reference. Receipt integrity, finalized payment, KYB identity and tax acceptance are separate checks; no business names or tax documents need to be published on-chain.

Browser note generation/encrypted backup, selected-field receipts and local FT withdrawal proving are implemented in Core/Front. The local prover verifies pinned assets and proof files, supports cancellation, and preserves the existing circuit; [Connected invoice checkout](https://github.com/Company-privacy-settlement/Core/blob/main/docs/INVOICE_PAYMENTS.md) supports original-group encrypted note selection → local proof → amount/fee review → separate wallet approvals → finalized confirmation, with selected-unit binding, cancellation, fresh merchant/status gates and conservative recovery. [Browser prover](https://github.com/Company-privacy-settlement/Core/blob/main/docs/BROWSER_PROVER.md). KYB is sample intake plus separately configured SDK issuer-signature checks, not live provider review or circuit-enforced identity admission. US/Abu Dhabi tax preparation and AI drafts are planned service work, not contract functions.

This development protocol has no production security audit, reviewed multi-party setup or funded public-network wallet validation. Permanently missing payout callbacks stay conservatively reserved; nullifiers must not be reset or destinations redirected to manufacture recovery. Variable denominations, automatic private group migration, cross-asset bridges, live KYB qualification and statutory filings remain future work.

## Specifications

- [Shared native NEAR v2, recovery and privacy](docs/SHARED.md)
- [USDC / NEP-141 v3](docs/USDC.md)
- [Independent pool v1 proof format](docs/PROTOCOL.md)
- [Independent factory v1](docs/FACTORY.md)
- [Distributed vault model](docs/DISTRIBUTED_VAULTS.md)
- [SDK/service privacy boundaries](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PRIVACY.md)

Payment dashboard validation on **2026-11-14**: **355 Core tests** and **182 desktop/mobile browser cases** passed, including 13 new Core and 18 browser dashboard cases; type checks, Node/browser SDK builds and the static Front build passed. Browser service/wallet fixtures do not establish a live public-network deployment.

Signed webhook validation on **2026-11-14**: **379 Core tests** and **194 desktop/mobile browser cases** passed, including 24 new Core and 12 browser webhook cases. Node/browser SDK, static build and compiled service/recovery checks passed. Tests use local index/wallet/transport fixtures; no real merchant callback or public backend deployment was performed.

Worker/contractor payout validation on **2026-11-14**: **401 Core tests** and **210 desktop/mobile browser cases** passed, including 22 new Core and 16 browser payout cases. Type checks, Node/browser SDK and static Front builds, compiled service restart/backup/recovery and documentation checks passed. Browser wallets, RPC and organization services are local fixtures; these results do not qualify funded public-network payroll or real mobile wallets.

[Sumsub KYB/KYC integration](https://github.com/Company-privacy-settlement/Core/blob/main/docs/VERIFICATION.md) is available in Organization → Verification: company/beneficial-owner review, secure supplemental submissions, encrypted minimal snapshots, authenticated invalidation and fixed review-date expiry/revocation. Activation needs an operator-contracted account, configured levels and HTTPS callbacks; development exercised provider fixtures, not real entities. The separate `/kyb/` sample stays unsubmitted.

KYB/KYC connector validation on **2026-11-14**: **428 Core tests** and **224 desktop/mobile browser cases** passed, including 27 new Core and 14 provider-workflow browser cases. Types, Node/browser SDK and static Front builds, compiled service restart/backup/recovery, documentation and SDK synchronization passed. Tests use local provider/RPC/wallet fixtures; no real Sumsub account, entity/person decision, live callback or public backend was qualified.
