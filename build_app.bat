@echo off
setlocal

echo Building release...
cargo build --release

if %ERRORLEVEL% neq 0 (
    echo Cargo build failed!
    exit /b %ERRORLEVEL%
)

echo Generating pico2w_shell.bin...
cargo objcopy --release -- -O binary pico2w_shell.bin

echo Generating pico2w_shell.hex...
cargo objcopy --release -- -O ihex pico2w_shell.hex

echo Packaging metadata...
python package_app.py pico2w_shell.bin shell_with_metadata.bin

echo Done! Output files:
echo - pico2w_shell.bin (Raw binary)
echo - pico2w_shell.hex (Intel HEX)
echo - shell_with_metadata.bin (Packaged for Bootloader)
