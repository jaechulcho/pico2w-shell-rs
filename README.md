# pico2w-shell-rs

Raspberry Pi Pico (RP2040) 및 Pico 2 (RP2350)를 위한 Rust 기반 베어메탈(Bare-metal) 펌웨어 예제 프로젝트입니다.

이 프로젝트는 `Embedded-hal`을 사용하여 GPIO 핀을 제어하고 LED를 점멸(Blinky)시키는 기능을 구현하며, `defmt`를 통한 디버그 로그 출력을 지원합니다.

## 주요 특징

- **멀티 아키텍처 지원**: Raspberry Pi Pico (RP2040, Cortex-M0+) 및 Pico 2 (RP2350, Cortex-M33 및 RISC-V)를 모두 지원합니다.
- **Bare-metal Rust**: OS 없이 돌아가는 순수 Rust 펌웨어 구현.
- **디버깅**: `defmt` 및 `RTT`를 통한 실시간 로깅 지원.
- **Picotool 연동**: `picotool info`를 통해 프로그램 이름, 버전 등의 메타데이터 확인 가능.

## 요구 사항

- **하드웨어**: Raspberry Pi Pico 또는 Pico 2 보드.
- **소프트웨어**:
  - Rust 툴체인 (최신 stable 또는 nightly)
  - `flip-link` (스택 오버플로우 보호)
  - `probe-rs` 또는 `picotool` (펌웨어 업로드용)
  - 각각의 타겟 아키텍처 지원:
    - `thumbv6m-none-eabi` (RP2040)
    - `thumbv8m.main-none-eabihf` (RP2350 Arm)
    - `riscv32imac-unknown-none-elf` (RP2350 RISC-V)

## 빌드 및 실행 방법

### 1. 타겟 추가
빌드하려는 아키텍처에 맞는 타겟을 추가합니다.
```bash
rustup target add thumbv6m-none-eabi # RP2040
rustup target add thumbv8m.main-none-eabihf # RP2350 Arm
rustup target add riscv32imac-unknown-none-elf # RP2350 RISC-V
```

### 2. 빌드
```bash
# RP2040 (Pico 1)
cargo build --release --target thumbv6m-none-eabi

# RP2350 (Pico 2 Arm)
cargo build --release --target thumbv8m.main-none-eabihf

# RP2350 (Pico 2 RISC-V)
cargo build --release --target riscv32imac-unknown-none-elf
```

### 3. 업로드 및 실행
`probe-rs`를 사용하거나 `elf2uf2-rs` 등을 사용하여 보드에 쓰기 작업을 수행합니다.

## 프로젝트 구조

- `src/main.rs`: 펌웨어 엔트리 포인트 및 주요 로직 (LED 점멸).
- `Cargo.toml`: 의존성 정의 및 타겟별 설정.
- `build.rs`: 빌드 시 필요한 설정 자동화.
- `rp2040.x`, `rp2350.x`: 링커 스크립트.

## 라이선스

이 프로젝트는 MIT 및 Apache-2.0 라이선스 하에 배포됩니다.
