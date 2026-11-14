# Contract — NEAR Core 정산

[English](README.md) · [한국어](README.ko.md) · [简体中文](README.zh-CN.md)

Core 프라이빗 기업 정산 프로토콜의 스마트 컨트랙트, Groth16 회로, 공개 벡터와 WASM 빌드 저장소입니다. [Core](https://github.com/Company-privacy-settlement/Core)는 증명·SDK·읽기 전용 API·인덱스, [Front](https://github.com/Company-privacy-settlement/Front)는 영문 기본 랜딩·Docs·지갑·서명 인보이스·증빙 화면을 담당합니다.

[서비스 개요](https://github.com/Company-privacy-settlement/Core/blob/main/docs/SERVICE.md) · [개발 로드맵](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ROADMAP.md) · [웹 Docs](https://core-private-settlement-0574c4.jiheon6768.chatgpt.site/docs/)

별도 [Core 기업 인보이스 API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_INVOICES.md)는 조직별 서명 인보이스 저장·조회, 중복 방지와 저장 시 암호화를 지원합니다. 오프체인 서비스 저장 기능이며 컨트랙트 ABI·수수료·결제 증명 검증은 기존과 같습니다.

[조직 결제 대시보드](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PAYMENT_DASHBOARD.md)는 서명 인보이스 검색·내보내기와 검증된 확정 출금 기반 조회를 지원합니다. 대기·실패 거래는 결제로 집계하지 않습니다. 컨트랙트 ABI·수수료·증명 권한은 유지됩니다.

## 구조와 버전

| 구성 요소 | 역할 |
| --- | --- |
| `contracts/forest` | 공유 노트 트리, 전역 nullifier, 지급 예약과 키 없는 볼트 생성 |
| `contracts/shared-vault` | 분리된 원금 보관, 입금 등록, 멱등 지급과 실패 환불 |
| `contracts/pool`, `contracts/factory` | v1 독립 풀·팩토리 호환 경로 |
| `circuits/spend.circom` | 실제 Groth16의 Poseidon 멤버십·nullifier·목적지/컨텍스트 바인딩 |

v2는 네이티브 NEAR, v3는 그룹마다 네이티브 USDC 등 하나의 NEP-141 토큰을 고정하고 증명 도메인에 바인딩합니다. v1은 독립 풀 호환 경로입니다. v1/v2는 멤버십 회로를 공유해도 도메인·액션 바인딩이 달라 노트·정책을 임의 교환할 수 없습니다.

원금은 개별 볼트에 남습니다. 공유 노트는 입금 볼트와 다른 볼트를 포함해 자금이 있는 한 볼트에서 직접 지급을 승인합니다. forest는 증명 상태·유동성 예약을 조정하며 원금 회수 권한은 없습니다. 원금을 여러 볼트에 순차 경유하지 않습니다. 작업에 운영자 서명이 필요 없고 자식 컨트랙트 배포에 키를 추가하지 않습니다. 공개 수취인·서명자·금액·볼트·참조·시간은 관찰 가능하며 볼트 수만으로 익명성을 보장하지 않습니다.

각 그룹은 고정 금액과 깊이 8 트리인 **총 256개 출력**을 사용하며 재사용 슬롯 256개가 아닙니다. 입금·비공개 전송은 출력을 사용하고 출금해도 복구되지 않습니다. 그룹마다 증명·프라이버시 영역은 분리됩니다. [공유 명세](docs/SHARED.md) · [프라이버시 한계](docs/SHARED.md#privacy-boundaries).

## 빌드와 테스트

Node.js 22 이상, 고정된 Rust 1.99.1, WASM 타깃과 C/C++ 빌드 도구가 필요합니다. 이 저장소에서 실행합니다.

```sh
npm ci
rustup target add wasm32-unknown-unknown
npm run build
npm test
```

빌드는 pool → factory → shared-vault → forest 순이며 부모 컨트랙트는 최적화한 자식 WASM을 포함합니다. `artifacts/`에 `core_pool.wasm`, `core_factory.wasm`, `core_shared_vault.wasm`, `core_forest.wasm`, `build-manifest.json`이 생성됩니다. 컨트랙트 테스트 전에 빌드하며 Core는 복사 전에 해시를 검증합니다.

```sh
# 단일 머신 개발 셋업. 새 셋업에는 새 검증키와 배포가 필요합니다.
npm run zk:setup
```

공개 증명·검증키·벡터만 커밋합니다. 비밀 노트·개인키·생성된 proving 아티팩트는 추적하지 않습니다. 이 개발 셋업은 검토된 다자간 ceremony가 아닙니다.

## 통합 CI

푸시·PR은 고정 Core 파이프라인에서 현재 Contract 커밋과 고정 Core/Front를 함께 검사합니다. 실제 WASM·단위·공유 벡터·Front SDK 최신성, Core/Front·브라우저 테스트, 네이티브/FT Groth16 샌드박스와 정확한 소스의 개발 릴리스를 검증합니다. 비공개 저장소에는 읽기 전용 체크아웃 자격 증명과 Core 재사용 워크플로 접근 설정이 필요합니다. [버전·증거·설정](https://github.com/Company-privacy-settlement/Core/blob/main/docs/CI.md). CI는 배포하거나 기존 검증키를 교체하지 않습니다.

## 로컬 체인 통합 검증

`Contract/`와 `Core/`를 형제 폴더로 체크아웃하고 Contract에서 실행합니다.

```sh
cd ../Core
npm ci
npm run contract:build
npm run zk:setup
(cd ../Contract && npm run build:test-ft)
npm run demo:ft
```

다른 컨트랙트 경로는 Core의 `CORE_CONTRACT_REPO`로 설정합니다. FT 샌드박스는 실제 WASM·Groth16과 **Circle USDC가 아닌 로컬 TESTUSD**를 사용합니다. 2026-11-13 최근 기록은 최종화 레코드 14개이며 수수료·복구, API·인덱스·저널, 수취인 준비, 브라우저 노트·영수증 백업과 최종 증빙 앵커를 다룹니다. Core의 `npm run demo:shared`는 네이티브 v2, 교차 볼트 지급, 동시 이중 지출 차단과 수취인 실패 재시도를 검증합니다. 일회성 체인 검증은 공개 네트워크 배포·실제 지갑 검증이 아닙니다.

## 자산과 추가 수수료

입금과 출금에 원금의 **0.15%를 각각 추가**하고 자산 최소 단위에서 내림합니다. 수취인은 원금을 전액 받습니다.

| 고정 단위 | 입금 | 별도 출금 지불 | 수취액 |
| --- | --- | --- | --- |
| 1 USDC | 1.0015 USDC | 0.0015 USDC | 1 USDC |
| 네이티브 v2 1 NEAR | 스토리지 포함 1.0215 NEAR | 스토리지 포함 0.0215 NEAR | 1 NEAR |

FT 그룹 해당 작업 스토리지는 0.02 NEAR이며 송신자/대상 컨트랙트별 최초 크레딧에 0.005 NEAR 원장 보증금이 붙습니다. 계정 활성화·토큰 등록·가스·FT 전송의 1 yocto 첨부는 별도입니다. 네이티브 v2 스토리지 0.02 NEAR는 registry 0.015 + vault 0.005로 나뉘며 추가 프로토콜 수수료가 아닙니다. 재시도·비공개 전송은 해당 스토리지만 내고 프로토콜 수수료를 다시 내지 않습니다. 입금 수수료는 볼트, 확정 출금 수수료는 forest에 귀속됩니다. 고정 treasury는 수익 수수료만 수령하고 원금·지급 대기 부채를 인출할 수 없습니다.

`new_ft`로 토큰이 고정된 별도 그룹을 생성합니다. 브라우저는 현재 설정된 테스트넷 FT 그룹을 지원하고 네이티브 NEAR는 Core CLI를 사용합니다. [USDC/NEP-141 명세·참조 주소](docs/USDC.md).

## 서비스 연동과 한계

Core의 별도 [Organization API](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATIONS.md)는 오프체인 조직 멤버·역할을 관리합니다. 조직 역할이 컨트랙트 서명, 노트 소유, 지급 목적지 변경이나 기업 신원 검증 권한을 부여하지 않습니다.

[조직 데이터 백업·복구](https://github.com/Company-privacy-settlement/Core/blob/main/docs/ORGANIZATION_BACKUPS.md)는 일관된 온라인 스냅샷, 세션·챌린지 폐기와 격리 복구를 제공합니다. 현재 권한을 별도 승인한 파일을 적용해야 새 운영 DB로 활성화되며, 과거 소유자·역할·대기 초대는 자동으로 권한에 복원하지 않습니다.

[기업 결제 승인](https://github.com/Company-privacy-settlement/Core/blob/main/docs/CORPORATE_PAYMENTS.md)은 작성자와 다른 Owner/Admin 승인자를 분리하고 결제·별도 0.15% 수수료·선행 호출을 고정합니다. 지원 지갑 흐름은 실행 직전 현재 멤버 권한과 승인을 검사하고 같은 호출을 한 번만 예약합니다. 조직 API·SDK·앱의 정책이며 기존 컨트랙트의 고정 수취인 증명 실행 권한은 변경하지 않습니다.

Core 인보이스 v2는 기존 ABI·회로로 결제 단위마다 다른 서명 참조를 바인딩합니다. 참조 중복·만료·결제 상한은 앱 검사이며 컨트랙트 예약이 아닙니다. 동시 결제·초과 입금이 가능합니다. 임의 영수증 루트로 서명된 인보이스 결제 참조를 교체할 수 없습니다. 영수증 무결성·최종 지급·KYB 신원·세무 인정은 별도 검사이며 기업명·세무 문서를 온체인에 공개할 필요는 없습니다.

Core/Front는 브라우저 노트 생성·암호화 백업·선택 필드 증빙과 로컬 FT 출금 증명을 지원합니다. 증명기는 고정한 파일·증명 파일 검증과 취소를 지원하며 기존 회로를 유지합니다. [인보이스 결제](https://github.com/Company-privacy-settlement/Core/blob/main/docs/INVOICE_PAYMENTS.md)에서 원래 그룹의 암호화 노트 선택 → 로컬 증명 → 금액·수수료 확인 → 개별 지갑 승인 → 최종화 결제 확인을 지원합니다. 선택 단위 고정, 취소, 서명 직전 기업 키·상태 재검사와 보수적 복구를 적용합니다. [브라우저 증명](https://github.com/Company-privacy-settlement/Core/blob/main/docs/BROWSER_PROVER.md). KYB는 샘플 입력과 별도로 구성한 SDK 발급자 서명 검사이며 실제 심사·회로 강제 신원 정책은 아닙니다. 미국/아부다비 세무 준비·AI 초안은 예정된 서비스 연동이며 컨트랙트 기능이 아닙니다.

프로덕션 보안 감사·검토된 다자 셋업·자금이 있는 공개 네트워크 지갑 검증은 아직 없습니다. 영구 누락된 지급 콜백은 보수적으로 예약 상태를 유지하며 nullifier 초기화·목적지 변경으로 복구를 만들어내면 안 됩니다. 가변 금액·자동 비공개 그룹 이전·자산 간 브리지·실제 KYB·법정 신고는 향후 범위입니다.

## 명세

- [네이티브 NEAR 공유 v2·복구·프라이버시](docs/SHARED.md)
- [USDC / NEP-141 v3](docs/USDC.md)
- [독립 풀 v1 증명 형식](docs/PROTOCOL.md)
- [독립 팩토리 v1](docs/FACTORY.md)
- [분산 볼트 모델](docs/DISTRIBUTED_VAULTS.md)
- [SDK/서비스 프라이버시](https://github.com/Company-privacy-settlement/Core/blob/main/docs/PRIVACY.md)

**2026-11-14 결제 대시보드 검증:** 신규 Core 13개·브라우저 18개를 포함한 **Core 355개 테스트**, **데스크톱·모바일 브라우저 182개 케이스**와 타입·Node/브라우저 SDK·정적 빌드를 통과했습니다. 브라우저의 서비스·지갑 픽스처 검증은 공개망 실배포를 의미하지 않습니다.
