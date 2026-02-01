# pico2w-shell-rs (Embassy Async Shell)

Pico 2 W (RP2350)를 위한 Rust 기반 비동기(Async) 펌웨어 프로젝트입니다.
이 버전은 **`pico2w-bootloader-rs`**와 함께 동작하도록 최적화되어 있습니다.

## 주요 기능

- **비동기 멀티태스킹**: `embassy-executor`를 통해 다중 태스크(LED 점멸, UART CLI 등) 실행.
- **부트로더 지원**: 커스텀 부트로더를 통한 안정적인 펌웨어 업데이트 및 무결성 검증(CRC32) 지원.
- **UART CLI 인터페이스**: 115200bps 통신 및 다양한 제어 명령어 제공.
- **CYW43 지원**: Wi-Fi 칩 제어 및 온보드 LED 제어 기능 포함.

## 하드웨어 및 메모리 맵

- **대상 보드**: Raspberry Pi Pico 2 W (RP2350)
- **메모리 구조**:
    | 영역 | 주소 범위 | 크기 | 설명 |
    | :--- | :--- | :--- | :--- |
    | **Bootloader** | `0x10000000` - `0x10010000` | 64KB | 시스템 부트로더 |
    | **Metadata** | `0x10010000` - `0x10010100` | 256B | 앱 정보 (Magic, CRC32 등) |
    | **Application** | `0x10010100` - | ~1.9MB | 실제 쉘 펌웨어 위치 |

## 빌드 및 배포 방법

부트로더를 사용하는 경우, 빌드 후 메타데이터를 포함하는 패키징 과정이 필요합니다.

### 1. 펌웨어 빌드
```bash
cargo build --release
```

### 2. 바이너리 변환 (ELF -> BIN)
```bash
cargo objcopy --release -- -O binary pico2w_shell.bin
```

### 3. 부트로더용 패키징 (CRC32 추가)
`package_app.py` 스크립트를 사용하여 부트로더가 인식할 수 있는 최종 바이너리를 생성합니다.
```bash
python package_app.py pico2w_shell.bin shell_with_metadata.bin
```

### 4. 업로드 (Flashing)
`probe-rs`를 사용하여 **Metadata 시작 주소(`0x10010000`)**에 업로드합니다.
```bash
probe-rs download --chip RP235x --binary-format binary --base-address 0x10010000 .\shell_with_metadata.bin
```

---

## 프로젝트 구조

- `src/main.rs`: 메인 비동기 로직 및 CLI 정의.
- `rp2350.x`: 부트로더 지원을 위해 수정된 링커 스크립트.
- `package_app.py`: 부트로더용 메타데이터 생성을 위한 Python 스크립트.
- `.cargo/config.toml`: RP2350 타겟 및 빌드 옵션 설정.

## 라이선스

이 프로젝트는 MIT 및 Apache-2.0 라이선스 하에 배포됩니다.
