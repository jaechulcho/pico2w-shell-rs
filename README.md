# pico2w-shell-rs (Embassy Async Shell)

Pico 2 W (RP2350)를 위한 Rust 기반 비동기(Async) 펌웨어 프로젝트입니다.
이 버전은 **`pico2w-bootloader-rs`**와 함께 동작하도록 최적화되어 있습니다.

## 주요 기능

- **비동기 멀티태스킹**: `embassy-executor`를 통해 다중 태스크(LED 점멸, UART CLI 등) 실행.
- **부트로더 지원**: 커스텀 부트로더를 통한 안정적인 펌웨어 업데이트 및 무결성 검증(CRC32) 지원.
- **원격 리셋**: `reboot` 명령어를 통해 소프트웨어적으로 부트로더에 진입할 수 있습니다.
- **UART CLI, TCP 터미널 및 Web Shell**: 시리얼 통신(115200bps) 뿐만 아니라, Wi-Fi 연결 후 TCP(8080) 단말 및 웹 브라우저(`http://192.168.4.1`)를 통한 원격 제어 인터페이스 제공. (접근 인증 포함)
- **Wi-Fi SoftAP 및 내부 DHCP**: 고유 UID를 기반으로 동적 액세스 포인트(SoftAP)를 생성하며, 연결되는 기기에 IP 주소 할당.
- **비휘발성 로깅 (LittleFS)**: 플래시 메모리 후반 2MB를 파일 시스템으로 구성하여 자동 회전(Log Rotation) 기반의 영구적 로그 기록 및 조회/삭제 기능(`log` 명령어) 지원.

## 하드웨어 및 메모리 맵

- **대상 보드**: Raspberry Pi Pico 2 W (RP2350)
- **메모리 구조**:
    | 영역            | 주소 범위                   | 크기   | 설명                      |
    | :-------------- | :-------------------------- | :----- | :------------------------ |
    | **Bootloader**  | `0x10000000` - `0x1000FFFF` | 64KB   | 시스템 부트로더           |
    | **Metadata**    | `0x10010000` - `0x10010FFF` | 4KB    | 앱 정보 (Magic, CRC32 등) |
    | **Application** | `0x10011000` - `0x101FFFFF` | ~1.9MB | 실제 쉘 펌웨어 위치       |
    | **FileSystem**  | `0x10200000` - `0x103FFFFF` | 2MB    | LittleFS 기반 로그 저장소 |

## CLI 명령어

시리얼 터미널(115200bps)을 통해 다음 명령어를 사용할 수 있습니다:
- `help`: 사용 가능한 명령어 목록 표시
- `led <on|off>`: GP28 LED 제어
- `info`: 시스템 정보 표시
- `echo <msg>`: 입력한 메시지를 그대로 반환
- `reboot`: **시스템을 리셋하여 부트로더로 진입** (원격 업데이트용)
- `auth <passkey>`: TCP 통신 및 웹 접속 시 보안 제어 권한 획득 (Passkey: `info`에 나오는 UID의 마지막 6자리)
- `log <level|command>`: 로그 레벨 지정 및 기록 관리
    - `log print`: 플래시에 저장된 전체 로그 출력
    - `log clear`: 플래시에 저장된 로그 전체 삭제
    - `log record <message>`: 사용자 지정 로그 메시지를 플래시에 기록
    - 출력 레벨 지정: `error`, `warn`, `info`, `debug`, `trace`
- `mkdir <path>`: 파일 시스템에 새로운 디렉토리 생성
- `cd <path>`: 현재 작업 디렉토리 변경
- `ls [path]`: 현재 혹은 지정한 디렉토리의 파일 및 폴더 목록 출력
- `cat <path>`: 지정한 파일의 내용을 터미널에 출력
- `cat <path>`: 지정한 파일의 내용을 터미널에 출력

## 무선 접속 가이드 (Wi-Fi)

1. **AP 연결**: 스마트폰 / PC 등 장치에서 와이파이 네트워크 검색 후 `Pico_2W_Shell_XXXXXXXX`(UID 기반) AP에 연결합니다. 
   - 비밀번호는 **UID의 마지막 8자리**입니다. (예: `E661385283XXXXXXYY` 이라면 `XXXXXXYY`)
2. **Web Shell 접속**: 브라우저에서 `http://192.168.4.1` 로 접속하여 명령어 입력이 가능합니다. 초기에 `auth <UID마지막6자리>` 를 입력해야 기능 제한이 풀립니다.
3. **TCP 터미널 접속**: `nc 192.168.4.1 8080` (Telnet/Netcat) 연결을 통해 원격 CLI 제어가 가능합니다.

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
