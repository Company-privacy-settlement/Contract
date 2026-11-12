# Contract

[English](README.md) · [한국어](README.ko.md) · [简体中文](README.zh-CN.md)

NEAR 프라이버시 정산 프로토콜의 **계약·ZK 회로 저장소**입니다. 지갑 SDK·CLI·서버 연동 코드는 [Core](https://github.com/Company-privacy-settlement/Core)에 분리합니다.

| 구성 | 역할 |
| --- | --- |
| `contracts/forest` | v2/v3 공통 노트 트리·전역 nullifier·지급 예약·keyless vault 생성 |
| `contracts/shared-vault` | v2/v3 원금 분산 보관·입금 등록·멱등 지급·실패 환불 |
| `contracts/pool`, `contracts/factory` | v1 독립 풀과 팩토리 호환 구현 |
| `circuits/spend.circom` | Poseidon membership·nullifier·지급 컨텍스트를 묶는 실제 Groth16 회로 |

v2는 **원금을 모으지 않고 익명성 집합을 공유**합니다. 예치금은 각 vault에 남고, forest는 공통 증명 상태만 관리합니다. A에 예치한 노트로 B의 가용 잔액에서 출금할 수 있으며, 어느 vault를 골라도 같은 nullifier는 한 번만 사용됩니다. 등록·출금·재시도에는 운영자 서명이 필요하지 않습니다.

## 빌드와 테스트

Node.js 22+, 고정 Rust toolchain 1.99.1, C/C++ 빌드 도구가 필요합니다.

```bash
npm ci
rustup target add wasm32-unknown-unknown
npm run build
npm test
```

빌드는 `pool → factory → shared-vault → forest` 순서로 진행합니다. 팩토리와 forest는 각각 최적화된 자식 vault WASM을 포함합니다. 생성 파일은 `artifacts/core_pool.wasm`, `core_factory.wasm`, `core_shared_vault.wasm`, `core_forest.wasm`, `build-manifest.json`입니다. 테스트 전에 빌드가 필요합니다. 계약에 키를 추가하는 배포 액션은 없습니다.

```bash
# 개발용 단일 머신 setup. 새 setup은 새 verifier와 새 배포가 필요합니다.
npm run zk:setup
```

검증 벡터에는 공개 증명과 공개 키만 포함합니다. 비밀 노트·개인키·생성된 proving artifacts는 커밋하지 않습니다. v1과 v2는 동일한 membership 회로를 사용하지만 **서로 다른 domain과 action binding**으로 구분됩니다.

## 실제 로컬 체인 검증

두 저장소를 형제 폴더로 체크아웃합니다. Core가 Contract 빌드 산출물의 SHA-256을 확인하고 복사합니다.

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

`demo:shared`는 2개 keyless vault의 3개 예치, A 출처 노트의 B 출금, 두 목적지를 향한 동시 이중 사용 방지, 없는 수신자에 대한 지급 실패·재시도, 0.15% 수수료 정산과 12개 확정 journal 기록을 검증합니다. 일회용 로컬 체인에서만 관리자 키를 제거하며, 공개 네트워크 배포·키 제거는 자동 실행하지 않습니다.

## USDC 지원

`new_ft`로 고정 USDC 계약 주소를 지정한 새 그룹을 만듭니다. 1 USDC 입금 시 **1.0015 USDC**, 출금 시 별도 **0.0015 USDC**를 받고 수신자는 **1 USDC 전액**을 받습니다. NEAR 가스·스토리지는 별도 통화로 처리합니다. 원금은 각 vault의 토큰 잔액에 분산하며, forest에는 출금 수수료만 들어갑니다. 정식 Circle USDC 주소와 테스트 절차는 [USDC 명세](docs/USDC.md)를 확인하세요. 공개 네트워크에 실제 배포한 상태는 아닙니다.

## 수수료와 범위

입금·출금 각각 원금의 0.15%를 **추가 납부**합니다. 출금자는 원금을 전액 받고, 보관 비용 0.02 NEAR와 가스는 별도입니다. v2 보관 비용은 registry 0.015 NEAR / vault 0.005 NEAR로 나누며 추가 청구가 아닙니다. 재시도는 보관 비용만 다시 냅니다. 입금 수수료는 해당 vault, 출금 수수료는 forest에서 확정 후 고정 treasury로만 회수할 수 있습니다. Forest에는 원금 회수 권한이 없습니다.

현재는 native NEAR v2와 **USDC/NEP-141 v3**를 지원하는 고정 denomination·공유 트리 깊이 8(총 output 256개)의 해커톤 코어입니다. 자산별 새 forest가 필요하며 v3는 토큰 계약 주소까지 증명 도메인에 고정합니다. 자동 비공개 마이그레이션, 가변 금액, cross-asset bridge, 법정 세무 신고, 실제 KYB 심사 서비스는 포함하지 않습니다. 단일 머신 setup과 새 회로/계약은 감사되지 않았으며 로컬·테스트넷 시연용입니다.

- [USDC/NEP-141 v3: 별도 통화 수수료·FT 등록·실패 복구](docs/USDC.md)
- [공유 익명성 v2·실패 복구 명세](docs/SHARED.md)
- [독립 풀 v1 증명 규격](docs/PROTOCOL.md)
- [독립 팩토리 v1](docs/FACTORY.md)
