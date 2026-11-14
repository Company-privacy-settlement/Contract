# Contract — NEAR Core 结算

[English](README.md) · [한국어](README.ko.md) · [简体中文](README.zh-CN.md)

Core 隐私企业结算协议的智能合约、Groth16 电路、公开向量及 WASM 构建仓库。[Core](https://github.com/Company-privacy-settlement/Core) 提供证明、SDK、只读 API 与索引；[Front](https://github.com/Company-privacy-settlement/Front) 提供默认英文的落地页、Docs、钱包、签名发票和证据工作区。

[服务概览](https://github.com/Company-privacy-settlement/Core/blob/main/docs/SERVICE.md) · [开发路线图](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ROADMAP.md) · [网页 Docs](https://core-private-settlement-0574c4.jiheon6768.chatgpt.site/docs/)

独立的 [Core 企业发票 API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_INVOICES.md) 现支持按组织存储不可变签名发票、幂等创建和静态加密。这是链下服务存储，合约 ABI、费用与付款证明验证保持不变。

## 架构与版本

| 组件 | 职责 |
| --- | --- |
| `contracts/forest` | 共享票据树、全局 nullifier、付款预留及无密钥金库创建 |
| `contracts/shared-vault` | 独立本金托管、存款登记、幂等支付与失败退款 |
| `contracts/pool`, `contracts/factory` | 兼容 v1 独立池与工厂 |
| `circuits/spend.circom` | 使用真实 Groth16 证明绑定 Poseidon 成员关系、nullifier 与目的地/上下文 |

v2 使用原生 NEAR。v3 为每个分组固定一种 NEP-141 代币（包括原生 USDC），并绑定至证明域。v1 保留独立池兼容路径。v1/v2 共享成员电路，但域和操作绑定不同，票据与策略不可任意互换。

本金保存在各独立金库。共享票据授权一个有余额的金库直接支付，可与原存款金库不同。forest 协调证明状态与流动性预留，没有归集本金权限。资金不会依次经过多个金库。操作无需运营者签名，子合约部署不会添加密钥。收款人、签名者、金额、金库、引用和时间仍公开；增加金库本身并不保证匿名性。

每组使用固定面额和深度 8 的树：**总计 256 个输出**，并非可复用的 256 个槽位。存款及私密转移消耗容量，提款不会恢复容量。各分组仍是独立的证明/隐私域。[共享规范](docs/SHARED.md) · [隐私边界](docs/SHARED.md#privacy-boundaries)。

## 构建与测试

需要 Node.js 22+、固定 Rust 1.99.1、WASM 目标及 C/C++ 构建工具。从本仓库运行：

```sh
npm ci
rustup target add wasm32-unknown-unknown
npm run build
npm test
```

构建顺序为 pool → factory → shared-vault → forest，父合约内嵌优化后的子合约 WASM。`artifacts/` 输出 `core_pool.wasm`、`core_factory.wasm`、`core_shared_vault.wasm`、`core_forest.wasm` 及 `build-manifest.json`。先构建再测试；Core 复制前验证哈希。

```sh
# 单机开发设置；新设置需要新的验证密钥和部署。
npm run zk:setup
```

仅提交公开证明、验证密钥及向量。秘密票据、私钥和生成的证明材料不得纳入版本控制。此开发设置并非经审查的多方仪式。

## 集成 CI

推送和 PR 调用固定 Core 提交的流水线，将当前 Contract 提交与固定 Core/Front 一起验证：实际 WASM/单元、共享向量、Front SDK 新鲜度、Core/Front 和浏览器测试、原生/FT Groth16 沙箱及准确源版本的开发发布包。私有仓库需要只读检出凭据和 Core 复用工作流访问设置。[版本、证据及配置](https://github.com/Company-privacy-settlement/Core/blob/main/docs/CI.md)。CI 不部署或替换现有验证密钥。

## 本地链集成验证

将 `Contract/` 与 `Core/` 作为相邻目录检出，从 Contract 执行：

```sh
cd ../Core
npm ci
npm run contract:build
npm run zk:setup
(cd ../Contract && npm run build:test-ft)
npm run demo:ft
```

可通过 Core 的 `CORE_CONTRACT_REPO` 指定其他合约目录。FT 沙箱使用真实 WASM/Groth16 与**本地 TESTUSD 测试代币，而非 Circle USDC**。2026-11-13 最近记录产生 14 条最终化记录，涵盖费用/恢复、API/索引/日志、收款人准备、浏览器票据/收据备份及最终证据锚定。在 Core 执行 `npm run demo:shared` 可验证原生 v2、跨金库支付、并发双花拒绝及收款失败重试。临时本地链演示并不部署公开网络，也不代表真实钱包验证。

## 资产与附加费用

存款与提款各**额外收取本金的 0.15%**，按资产最小单位向下取整。收款人获得全部本金。

| 固定面额 | 存款 | 另付提款费用 | 收款 |
| --- | --- | --- | --- |
| 1 USDC | 1.0015 USDC | 0.0015 USDC | 1 USDC |
| 原生 v2 1 NEAR | 含存储 1.0215 NEAR | 含存储 0.0215 NEAR | 1 NEAR |

FT 分组适用操作存储为 0.02 NEAR，发送者/目标合约首次信用另有 0.005 NEAR 账本押金。账户激活、代币注册、gas 及 FT 转账附带的 1 yocto 单独计算。原生 v2 的 0.02 NEAR 存储分为 registry 0.015 + vault 0.005，并非额外协议费。重试/私密转移支付适用存储，不重复收取协议费。存款费留在金库，已确认提款费归 forest。固定 treasury 只能收取已赚取费用，不能提取本金或待支付负债。

`new_ft` 创建独立且固定代币的分组。浏览器目前支持已配置的测试网 FT 分组，原生 NEAR 使用 Core CLI。[USDC/NEP-141 规范与参考地址](docs/USDC.md)。

## 服务集成与边界

Core 独立的 [Organization API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATIONS.md) 管理链下团队成员和角色。组织角色不授予合约签名、票据所有权、付款重定向或已验证企业身份。

[组织数据备份与恢复](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_BACKUPS.md)提供一致在线快照、会话/挑战废弃及隔离恢复。必须使用单独审核的当前权限文件才能激活到新的运行数据库；历史所有者、角色和待处理邀请不会自动恢复为权限。

[企业付款审批](https://github.com/Company-privacy-settlement/Core/blob/main/docs/CORPORATE_PAYMENTS.md)分离创建者与另一位 Owner/Admin 审批者，固定付款、额外 0.15% 费用及前置调用。支持的钱包流程在执行前复查当前成员权限与审批，并将每个准确调用仅保留一次。这属于组织 API/SDK/应用策略；现有合约继续支持无许可的固定收款人证明执行。

Core 发票 v2 使用现有 ABI/电路，为每个支付单位绑定独立签名引用。引用唯一性、有效期及支付上限均为应用检查，并非合约预留，仍可能并发或超额付款。任意收据根不能替换签名发票支付引用。收据完整性、最终付款、KYB 身份和税务认可分别验证，无需在链上发布企业名称或税务文件。

Core/Front 已支持浏览器票据生成/加密备份、选定字段证据及本地 FT 提现证明。证明器验证固定材料与证明文件，支持取消并保持现有电路；[发票结账](https://github.com/Company-privacy-settlement/Core/blob/main/docs/INVOICE_PAYMENTS.md)支持原分组加密票据选择 → 本地证明 → 金额与费用确认 → 逐项钱包授权 → 最终付款确认，并提供所选单位绑定、取消、签名前商户密钥/状态复查与保守恢复。[浏览器证明](https://github.com/Company-privacy-settlement/Core/blob/main/docs/BROWSER_PROVER.md)。KYB 为示例资料流程与另行配置的 SDK 签发者签名检查，并非真实审核或电路强制身份准入。美国/阿布扎比税务准备与 AI 草稿是计划中的服务集成，不是合约功能。

协议尚无生产安全审计、经审查的多方设置或已注资公开网络钱包验证。永久缺失付款回调时保守保留预留状态；不得通过重置 nullifier 或更改目的地制造恢复。可变金额、自动私密分组迁移、跨资产桥、真实 KYB 与法定申报属于后续工作。

## 规范

- [原生 NEAR 共享 v2、恢复及隐私](docs/SHARED.md)
- [USDC / NEP-141 v3](docs/USDC.md)
- [独立池 v1 证明格式](docs/PROTOCOL.md)
- [独立工厂 v1](docs/FACTORY.md)
- [分布式金库模型](docs/DISTRIBUTED_VAULTS.md)
- [SDK/服务隐私边界](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PRIVACY.md)
