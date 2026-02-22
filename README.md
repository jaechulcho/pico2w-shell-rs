# pico2w-shell-rs (Embassy Async Shell)

Pico 2 W (RP2350)를 위한 Rust 기반 비동기(Async) 펌웨어 프로젝트입니다.
이 버전은 **`pico2w-bootloader-rs`**와 함께 동작하도록 최적화되어 있습니다.

## 주요 기능

- **비동기 멀티태스킹**: `embassy-executor`를 통해 다중 태스크(LED 점멸, UART CLI 등) 실행.
- **부트로더 지원**: 커스텀 부트로더를 통한 안정적인 펌웨어 업데이트 및 무결성 검증(CRC32) 지원.
- **원격 리셋**: `reboot` 명령어를 통해 소프트웨어적으로 부트로더에 진입할 수 있습니다.
- **UART CLI 인터페이스**: 115200bps 통신 및 다양한 제어 명령어 제공.

## 하드웨어 및 메모리 맵

- **대상 보드**: Raspberry Pi Pico 2 W (RP2350)
- **메모리 구조**:
    | 영역            | 주소 범위                   | 크기   | 설명                      |
    | :-------------- | :-------------------------- | :----- | :------------------------ |
    | **Bootloader**  | `0x10000000` - `0x10010000` | 64KB   | 시스템 부트로더           |
    | **Metadata**    | `0x10010000` - `0x10011000` | 4KB    | 앱 정보 (Magic, CRC32 등) |
    | **Application** | `0x10010100` -              | ~1.9MB | 실제 쉘 펌웨어 위치       |

## CLI 명령어

시리얼 터미널(115200bps)을 통해 다음 명령어를 사용할 수 있습니다:
- `help`: 사용 가능한 명령어 목록 표시
- `led <on|off>`: GP28 LED 제어
- `info`: 시스템 정보 표시
- `reboot`: **시스템을 리셋하여 부트로더로 진입** (원격 업데이트용)

## 빌드 및 배포 방법

### 1. 펌웨어 빌드
```bash
cargo build --release
```

### 2. 업로드 (자동화된 방법 - 추천)
`pico2w-downloader-rs`를 사용하면 리셋부터 업데이트까지 자동으로 수행됩니다.
```bash
# downloader 프로젝트 디렉토리에서
cargo run --release -- <COM_PORT> ../pico2w-shell-rs/target/thumbv8m.main-none-eabihf/release/pico2w-shell-rs.hex --reboot
```

### 3. 업로드 (수동 패키징 방법)
1. 바이너리 변환: `cargo objcopy --release -- -O binary pico2w_shell.bin`
2. 메타데이터 추가: `python package_app.py pico2w_shell.bin shell_with_metadata.bin`
3. 플래싱: `probe-rs download --chip RP235x --binary-format binary --base-address 0x10010000 .\shell_with_metadata.bin`

---

## 라이선스

이 프로젝트는 MIT 및 Apache-2.0 라이선스 하에 배포됩니다.
