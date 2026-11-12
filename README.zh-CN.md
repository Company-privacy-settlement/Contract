# Contract

[English](README.md) · [한국어](README.ko.md) · [简体中文](README.zh-CN.md)

共享票据授权一个有流动性的金库直接向收款人付款，本金不会连续经过多个金库。公开元数据仍可关联付款，增加金库本身不保证匿名性。Core 发票条款 v2 使用现有合约 ABI 和电路，为每个付款单位提供不同引用。引用复用检查属于应用层，不是合约预留功能。参见[隐私边界](docs/SHARED.md#privacy-boundaries)。

这是 NEAR 隐私结算协议的**智能合约与 ZK 电路仓库**。钱包 SDK、CLI 和服务器集成代码单独存放在 [Core](https://github.com/Company-privacy-settlement/Core) 中。

| 组件 | 职责 |
| --- | --- |
| `contracts/forest` | v2/v3 共享票据树、全局 nullifier、付款预留和无密钥 vault 创建 |
| `contracts/shared-vault` | v2/v3 本金分散保管、入金登记、幂等付款和失败退款 |
| `contracts/pool`、`contracts/factory` | 兼容的 v1 独立池与工厂实现 |
| `circuits/spend.circom` | 将 Poseidon 成员关系、nullifier 与付款上下文绑定的真实 Groth16 电路 |

v2 **在不集中本金的前提下共享匿名集**。存入的资金留在各个 vault 中，forest 只协调共享证明状态。存入 A 的票据可以从 B 的可用流动性中提款；无论选择哪个 vault，同一个 nullifier 都只能使用一次。登记、提款和重试均不需要运营方签名。

## 构建与测试

需要 Node.js 22 及以上版本、固定的 Rust 1.99.1 工具链，以及 C/C++ 构建工具。

```bash
npm ci
rustup target add wasm32-unknown-unknown
npm run build
npm test
```

构建顺序为 `pool → factory → shared-vault → forest`。工厂和 forest 分别嵌入优化后的子 vault WASM。生成文件包括 `artifacts/core_pool.wasm`、`core_factory.wasm`、`core_shared_vault.wasm`、`core_forest.wasm` 和 `build-manifest.json`。运行合约测试前需要先构建。部署操作不会向子合约添加密钥。

```bash
# 开发用单机可信设置；重新设置需要新的验证器和部署。
npm run zk:setup
```

测试向量仅包含公开证明和验证密钥。秘密票据、私钥及生成的证明产物不提交到仓库。v1 和 v2 使用相同的成员关系电路，但采用**不同的域和操作绑定**。

## 本地链验证

将两个仓库检出到同级目录。Core 会验证 Contract 构建产物的 SHA-256，然后再复制。

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

`demo:shared` 验证两个无密钥 vault 中的三笔入金、源自 A 的票据从 B 提款、面向不同付款目的地的并发双花防护、向不存在的收款账户付款失败后的重试、0.15% 手续费结算，以及 12 条最终确认的 journal 记录。管理员密钥仅在一次性本地链中移除；不会自动执行公开网络部署或密钥移除。

## USDC 支持

通过 `new_ft` 创建固定 USDC 代币合约地址的新组。存入 1 USDC 时转入 **1.0015 USDC**；提款时另付 **0.0015 USDC**，收款人收到**完整的 1 USDC**。NEAR gas 和存储费用按独立币种处理。本金分散在各 vault 的代币余额中，只有提款手续费进入 forest。Circle 原生 USDC 地址和测试步骤见 [USDC 规范](docs/USDC.md)。当前实现尚未部署到公开网络。

## 手续费与范围

入金和提款均在本金之外**加收 0.15%**。收款人获得全额本金，0.02 NEAR 存储费用和 gas 另计。v2 将存储费用分配给 registry（0.015 NEAR）和 vault（0.005 NEAR），不是额外收费。重试仅再次支付存储费用。入金手续费在对应 vault 中累计，已确认的提款手续费在 forest 中累计；已赚取的手续费只能转入初始化时固定的 treasury。Forest 没有提取本金的权限。

这是支持原生 NEAR v2 与 **USDC/NEP-141 v3** 的黑客松核心实现，采用固定面额和深度为 8 的共享树（总计 256 个输出）。每种资产需要独立的 forest，v3 还将代币合约地址固定在证明域中。不包含自动隐私迁移、可变金额、跨资产桥接、法定税务申报或真实 KYB 审查服务。单机可信设置及电路/合约未经生产安全审计，仅用于本地链和测试网演示。

- [USDC/NEP-141 v3：独立币种手续费、FT 注册与失败恢复](docs/USDC.md)
- [共享匿名集 v2 与失败恢复规范](docs/SHARED.md)
- [独立池 v1 证明格式](docs/PROTOCOL.md)
- [独立工厂 v1](docs/FACTORY.md)
