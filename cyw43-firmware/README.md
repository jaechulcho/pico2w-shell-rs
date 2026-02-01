# CYW43 Firmware

Pico 2 W의 온보드 LED 및 Wi-Fi 기능을 사용하려면 CYW43 무선 칩용 펌웨어가 필요합니다.

1. [embassy-rp](https://github.com/embassy-rs/embassy/tree/main/cyw43-firmware) 리포지토리 또는 관련 소스에서 아래 파일들을 다운로드하세요:
   - `43439A0.bin`
   - `43439A0_clm.bin`
2. 다운로드한 파일들을 이 디렉토리(`cyw43-firmware/`)에 넣으세요.

파일이 없으면 빌드 시 `include_bytes!` 매크로에서 오류가 발생합니다.
