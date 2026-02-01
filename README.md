# pico2w-shell-rs (Embassy Async Shell)

Pico 2 W (RP2350)를 위한 Rust 기반 비동기(Async) 펌웨어 프로젝트입니다.
Embassy 프레임워크를 사용하여 백그라운드 태스크와 실시간 CLI 인터페이스를 동시에 구동하는 구조를 보여줍니다.

## 주요 기능

- **비동기 멀티태스킹**: `embassy-executor`를 통해 다중 태스크(LED 점멸, UART CLI 등)를 효율적으로 실행합니다.
- **듀얼 LED 제어**:
    - **CYW43 LED**: Wi-Fi 칩 상의 LED가 백그라운드 루프(`blink_task`)에서 500ms 간격으로 자동 점멸합니다.
    - **사용자 제어 LED (GP28)**: UART 명령어를 통해 사용자가 직접 끄고 켤 수 있습니다.
- **UART CLI 인터페이스**:
    - 115200bps 비동기 통신 (UART0, RX: PIN 1, TX: PIN 0).
    - 명령어: `help`, `led on`, `led off`, `info`.
- **CYW43 지원**: Pico 2 W의 하드웨어를 제어하기 위한 PIO SPI 드라이버 및 펌웨어 로드 로직 포함.

## 요구 사항

- **하드웨어**: Raspberry Pi Pico 2 W (RP2350)
- **소프트웨어**:
  - Rust 툴체인 (stable v1.8x 이상 추천)
  - 타겟: `thumbv8m.main-none-eabihf` (RP2350 Arm Cortex-M33)
  - `probe-rs` 또는 `picotool` (업로드용)
- **펌웨어 바이너리**: `cyw43-firmware/` 디렉토리에 필수 펌웨어 파일이 필요합니다. (자세한 내용은 해당 디렉토리의 `README.md` 참고)

## 빌드 및 실행 방법

### 1. 타겟 추가
```bash
rustup target add thumbv8m.main-none-eabihf
```

### 2. 빌드
```bash
cargo build --release
```

### 3. 업로드 및 실행
`probe-rs`를 사용하는 경우:
```bash
cargo run --release
```
*(자동으로 빌드 후 보드에 기록하고 defmt 로그를 출력합니다.)*

## 프로젝트 구조

- `src/main.rs`: Embassy 기반 엔트리 포인트 및 비동기 태스크 정의.
- `Cargo.toml`: Embassy 및 CYW43 관련 의존성 설정.
- `cyw43-firmware/`: 무선 칩 구동을 위한 펌웨어/NVRAM 바이너리 보관 장소.
- `.cargo/config.toml`: 링크 설정 및 실행기(`probe-rs`) 정의.

## 라이선스

이 프로젝트는 MIT 및 Apache-2.0 라이선스 하에 배포됩니다.
