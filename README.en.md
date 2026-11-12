# Contract

[한국어](README.md) · [简体中文](README.zh-CN.md) · [English](README.en.md)

The **smart contract and ZK circuit repository** for the NEAR privacy settlement protocol. Wallet SDK, CLI, and server integration code live separately in [Core](https://github.com/Company-privacy-settlement/Core).

| Component | Role |
| --- | --- |
| `contracts/forest` | Shared v2/v3 note tree, global nullifiers, payout reservations, and keyless vault creation |
| `contracts/shared-vault` | Distributed v2/v3 principal custody, deposit registration, idempotent payouts, and failure refunds |
| `contracts/pool`, `contracts/factory` | Compatible v1 independent pools and factory |
| `circuits/spend.circom` | Real Groth16 circuit binding Poseidon membership, nullifiers, and the payout context |

V2 **shares an anonymity set without pooling principal**. Deposits remain in individual vaults; the forest coordinates shared proof state. A note deposited in A can withdraw from B's available liquidity, and the same nullifier can be spent only once across all vaults. Registration, withdrawal, and retry operations require no operator signature.

## Build and test

Requires Node.js 22+, the pinned Rust 1.99.1 toolchain, and C/C++ build tools.

```bash
npm ci
rustup target add wasm32-unknown-unknown
npm run build
npm test
```

Build order is `pool → factory → shared-vault → forest`. The factory and forest embed the optimized child vault WASM. Outputs are `artifacts/core_pool.wasm`, `core_factory.wasm`, `core_shared_vault.wasm`, `core_forest.wasm`, and `build-manifest.json`. Build before running contract tests. Deployment actions do not add keys to child contracts.

```bash
# Single-machine development setup. A new setup requires a new verifier and deployment.
npm run zk:setup
```

Test vectors contain only public proofs and verification keys. Secret notes, private keys, and generated proving artifacts are not committed. V1 and v2 use the same membership circuit but **different domains and action bindings**.

## Local-chain validation

Check out both repositories as siblings. Core verifies the SHA-256 hashes of Contract build artifacts before copying them.

```text
workspace/
  Contract/
  Core/
```

```bash
cd ../Core
npm ci
npm run contract:build
npm run zk:setup
npm run demo:shared
```

`demo:shared` validates three deposits across two keyless vaults, an A-origin note withdrawing from B, concurrent double-spend prevention across two destinations, failed payouts to a nonexistent recipient and subsequent retries, 0.15% fee accounting, and 12 finalized journal records. Administrator keys are removed only in the disposable local chain; public-network deployment and key removal are not performed automatically.

## USDC support

Use `new_ft` to create a new group pinned to a USDC token contract. Depositing 1 USDC sends **1.0015 USDC**; withdrawal pays a separate **0.0015 USDC** fee, while the recipient receives the **full 1 USDC**. NEAR gas and storage are accounted for separately. Principal stays in each vault's token balance; only withdrawal fees enter the forest. See the [USDC specification](docs/USDC.md) for Circle's native USDC addresses and test procedures. This implementation has not been deployed to a public network.

## Fees and scope

Both deposits and withdrawals charge **an additional 0.15% of principal**. Recipients receive the full principal; 0.02 NEAR storage and gas are separate. V2 divides the storage charge between registry (0.015 NEAR) and vault (0.005 NEAR); this is not an extra charge. Retries pay storage only. Deposit fees accrue in the vault, and confirmed withdrawal fees accrue in the forest. Earned fees can be collected only to the fixed treasury. The forest has no principal-sweep authority.

This hackathon core supports native NEAR v2 and **USDC/NEP-141 v3**, fixed denominations, and a shared tree of depth 8 (256 total outputs). Each asset requires a separate forest; v3 also pins the token contract in the proof domain. Automatic private migration, variable amounts, cross-asset bridges, statutory tax filing, and real KYB review services are outside the implementation. The single-machine setup and circuits/contracts have not undergone a production audit and are intended for local/testnet demonstrations.

- [USDC/NEP-141 v3: separate-currency fees, FT registration, and recovery](docs/USDC.md)
- [Shared anonymity v2 and recovery specification](docs/SHARED.md)
- [Independent pool v1 proof format](docs/PROTOCOL.md)
- [Independent factory v1](docs/FACTORY.md)
