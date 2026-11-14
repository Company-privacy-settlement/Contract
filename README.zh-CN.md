# Contract — NEAR Core 结算

[English](README.md) · [한국어](README.ko.md) · [简体中文](README.zh-CN.md)

Core 隐私企业结算协议的智能合约、Groth16 电路、公开向量及 WASM 构建仓库。[Core](https://github.com/Company-privacy-settlement/Core) 提供证明、SDK、只读 API 与索引；[Front](https://github.com/Company-privacy-settlement/Front) 提供默认英文的落地页、Docs、钱包、签名发票和证据工作区。

[服务概览](https://github.com/Company-privacy-settlement/Core/blob/main/docs/SERVICE.md) · [开发路线图](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ROADMAP.md) · [网页 Docs](https://core-private-settlement-0574c4.jiheon6768.chatgpt.site/docs/)

独立的 [Core 企业发票 API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_INVOICES.md) 现支持按组织存储不可变签名发票、幂等创建和静态加密。这是链下服务存储，合约 ABI、费用与付款证明验证保持不变。

[组织付款仪表板](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PAYMENT_DASHBOARD.md) 提供签名发票搜索/导出，并从已验证的最终化成功提款计算观察状态。待处理和失败尝试不计入付款，合约 ABI、费用和证明权限不变。

独立的 [付款 Webhook 服务](https://github.com/Company-privacy-settlement/Core/blob/main/docs/WEBHOOKS.md) 使用已验证最终成功提款及关联交易来源。HMAC 事件、持久重试和接收端去重属于服务功能，合约 ABI、资金保管和证明权限不变。

[员工/承包商批量流程](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PAYOUT_BATCHES.md) 编排现有单票据提款、逐收款人审批、附加费用与固定目标恢复，不增加集中薪资钱包、原子多调用 ABI 或组织托管权限。

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

Core/Front 已支持浏览器票据生成/加密备份、选定字段证据及本地 FT 提现证明。证明器验证固定材料与证明文件，支持取消并保持现有电路；[发票结账](https://github.com/Company-privacy-settlement/Core/blob/main/docs/INVOICE_PAYMENTS.md)支持原分组加密票据选择 → 本地证明 → 金额与费用确认 → 逐项钱包授权 → 最终付款确认，并提供所选单位绑定、取消、签名前商户密钥/状态复查与保守恢复。[浏览器证明](https://github.com/Company-privacy-settlement/Core/blob/main/docs/BROWSER_PROVER.md)。KYB 为示例资料流程与另行配置的 SDK 签发者签名检查，并非真实审核或电路强制身份准入。美国/阿布扎比有限规则及来源绑定 AI 草稿已在服务、SDK 与 Front 中实现，不改变合约 ABI 或电路。

协议尚无生产安全审计、经审查的多方设置或已注资公开网络钱包验证。永久缺失付款回调时保守保留预留状态；不得通过重置 nullifier 或更改目的地制造恢复。可变金额、自动私密分组迁移、跨资产桥、真实 KYB 运行验证与法定申报属于后续工作。

## 规范

- [原生 NEAR 共享 v2、恢复及隐私](docs/SHARED.md)
- [USDC / NEP-141 v3](docs/USDC.md)
- [独立池 v1 证明格式](docs/PROTOCOL.md)
- [独立工厂 v1](docs/FACTORY.md)
- [分布式金库模型](docs/DISTRIBUTED_VAULTS.md)
- [SDK/服务隐私边界](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PRIVACY.md)

**2026-11-14 付款仪表板验证：**包含新增 13 项 Core 和 18 项浏览器用例的 **355 项 Core 测试**与 **182 项桌面/移动浏览器用例**通过，类型、Node/浏览器 SDK 和静态构建也通过。浏览器服务/钱包测试夹具不代表公开网络实际部署。

**2026-11-14 付款 Webhook 验证：**含新增 24 项 Core 和 12 项浏览器用例的 **379 项 Core 测试**与 **194 项桌面/移动浏览器用例**通过，Node/浏览器 SDK、静态构建和编译服务恢复检查通过。使用本地索引/钱包/传输夹具，未向实际商户回调发送消息或部署公开后端。

员工与承包商批量付款验证（**2026-11-14**）：**401 项 Core 测试**、**210 项桌面/移动浏览器用例**通过，其中新增 22 项 Core 和 16 项批量付款浏览器用例。类型检查、Node/浏览器 SDK、静态 Front 构建、编译服务重启/备份/恢复及文档检查均通过。钱包、RPC 和组织服务使用本地测试环境；结果不代表已验证真实资金的公网工资支付或真实移动钱包。

Organization → Verification 新增 [Sumsub KYB/KYC 集成](https://github.com/Company-privacy-settlement/Core/blob/main/docs/VERIFICATION.md)，支持企业/实益所有人审核、安全补充资料、最小状态加密、签名回调及基于审核日期的到期/撤销。启用需要签约账户、审核级别和 HTTPS 回调；开发验证使用提供方测试环境模拟，未审核真实企业。独立 `/kyb/` 示例仍不提交。

KYB/KYC 集成验证（**2026-11-14**）：**428 项 Core 测试**和 **224 项桌面/移动浏览器用例**通过，含新增 27 项 Core 和 14 项提供方流程浏览器用例。类型、Node/浏览器 SDK、静态 Front 构建、编译服务重启/备份/恢复、文档及 SDK 同步检查通过。使用本地提供方/RPC/钱包测试环境；未验证真实 Sumsub 账户、企业/个人审核、实际回调或公网后端。

[美国/阿布扎比规则引擎](https://github.com/Company-privacy-settlement/Core/blob/main/docs/RULES.md)按辖区、员工/承包商/B2B 关系及适用日期选择 7 项已核对来源的组件候选规则，保留精确算式与官方依据。浏览器/CLI 本地运行；审核模式需要独立可信的 Ed25519 审批。未附专业审核批准、完整税额、自动预扣或申报。

规则引擎验证（**2026-11-14**）：**459 项 Core 测试**和 **240 项桌面/移动浏览器用例**通过，含新增 31 项 Core 和 16 项 Rules 浏览器用例。类型、Node/浏览器 SDK、静态 Front 构建、编译 CLI 导出/复核、服务重启/备份/恢复及来源/文档检查通过。审核密钥为测试数据；未进行专业税务/法律规则批准、真实纳税人评估、自动预扣或申报。

[AI 税务与程序草稿](https://github.com/Company-privacy-settlement/Core/blob/main/docs/TAX_DRAFTS.md)位于 Organization → Tax drafts：验证规则计算与选定收据值的精确绑定、带来源草稿、组织加密存储、独立审核、不可变修订、撤销及 JSON/Markdown 导出。AI 仅排列已验证语句 ID，不接收金额或证据；确定性提纲明确区分。真实 AI 启用需要私有提供商密钥与模型，有效审核需要运营方固定的签名规则策略。不附带专业规则批准、自主法律建议、法定申报或付款授权。

草稿专项验证（**2026-11-14**）通过：**488 项 Core 测试**（新增草稿 29 项）、**16 项桌面/移动草稿浏览器用例**、3 项 Front 单元、类型检查、Node/浏览器 SDK 与静态构建、编译服务/备份/恢复及文档/来源清单检查。AI 传输与审核签名使用受控测试环境，未执行真实模型调用、专业规则批准或法定申报。
