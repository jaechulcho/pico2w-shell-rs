use core::sync::atomic::{AtomicBool, Ordering};
use embassy_rp::gpio::Output;
use embassy_rp::uart::{Async, UartTx};
//use embedded_io_async::Write;

use crate::ble;

// 모든 커맨트 인스턴스를 모아둔 배열
pub enum CommandEnum {
    Help(HelpCommand),
    Led(LedCommand),
    Info(InfoCommand),
    Echo(EchoCommand),
    Log(LogCommand),
    Auth(AuthCommand),
    Reboot(RebootCommand),
    Mkdir(MkdirCommand),
    Cd(CdCommand),
    Ls(LsCommand),
    Cat(CatCommand),
}

impl CommandEnum {
    // 트레이트 메서드를 대리 호출(Delegate)
    fn name(&self) -> &str {
        match self {
            Self::Help(c) => c.name(),
            Self::Led(c) => c.name(),
            Self::Info(c) => c.name(),
            Self::Echo(c) => c.name(),
            Self::Log(c) => c.name(),
            Self::Auth(c) => c.name(),
            Self::Reboot(c) => c.name(),
            Self::Mkdir(c) => c.name(),
            Self::Cd(c) => c.name(),
            Self::Ls(c) => c.name(),
            Self::Cat(c) => c.name(),
        }
    }

    fn description(&self) -> &str {
        match self {
            Self::Help(c) => c.description(),
            Self::Led(c) => c.description(),
            Self::Info(c) => c.description(),
            Self::Echo(c) => c.description(),
            Self::Log(c) => c.description(),
            Self::Auth(c) => c.description(),
            Self::Reboot(c) => c.description(),
            Self::Mkdir(c) => c.description(),
            Self::Cd(c) => c.description(),
            Self::Ls(c) => c.description(),
            Self::Cat(c) => c.description(),
        }
    }

    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        led: &mut Output<'static>,
        args: &[&str],
        uid_str: &str,
    ) {
        match self {
            Self::Help(c) => c.exec(uart, led, args, uid_str).await,
            Self::Led(c) => c.exec(uart, led, args, uid_str).await,
            Self::Info(c) => c.exec(uart, led, args, uid_str).await,
            Self::Echo(c) => c.exec(uart, led, args, uid_str).await,
            Self::Log(c) => c.exec(uart, led, args, uid_str).await,
            Self::Auth(c) => c.exec(uart, led, args, uid_str).await,
            Self::Reboot(c) => c.exec(uart, led, args, uid_str).await,
            Self::Mkdir(c) => c.exec(uart, led, args, uid_str).await,
            Self::Cd(c) => c.exec(uart, led, args, uid_str).await,
            Self::Ls(c) => c.exec(uart, led, args, uid_str).await,
            Self::Cat(c) => c.exec(uart, led, args, uid_str).await,
        }
    }
}

const COMMANDS: &[CommandEnum] = &[
    CommandEnum::Help(HelpCommand),
    CommandEnum::Led(LedCommand),
    CommandEnum::Info(InfoCommand),
    CommandEnum::Echo(EchoCommand),
    CommandEnum::Log(LogCommand),
    CommandEnum::Auth(AuthCommand),
    CommandEnum::Reboot(RebootCommand),
    CommandEnum::Mkdir(MkdirCommand),
    CommandEnum::Cd(CdCommand),
    CommandEnum::Ls(LsCommand),
    CommandEnum::Cat(CatCommand),
];

static BLE_AUTHENTICATED: AtomicBool = AtomicBool::new(false);

pub async fn uart_write_all(uart: &mut UartTx<'static, Async>, buf: &[u8]) {
    if buf.is_empty() {
        return;
    }
    let _ = uart.write(buf).await;
    // Removed `uart.blocking_flush()` to avoid busy-waiting and blocking other async tasks.

    // Also send to BLE TX Channel
    if !buf.is_empty() {
        for chunk in buf.chunks(64) {
            let mut vec: heapless::Vec<u8, 64> = heapless::Vec::new();
            if vec.extend_from_slice(chunk).is_ok() {
                let _ = ble::BLE_TX_CHANNEL.try_send(vec);
            }
        }
    }
}

pub trait Command {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        led: &mut Output<'static>,
        args: &[&str],
        uid_str: &str,
    );
}

pub struct HelpCommand;
impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }
    fn description(&self) -> &str {
        "Show this help"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        _args: &[&str],
        _uid_str: &str,
    ) {
        uart_write_all(uart, b"Available commands:\r\n").await;

        for cmd in COMMANDS {
            // 각 커맨트의 name()과 description()을 재사용
            uart_write_all(uart, b"  ").await;
            uart_write_all(uart, cmd.name().as_bytes()).await;

            // 간격을 맞추기 위한 공백 처리 (예, 12칸 정렬)
            let padding = 12usize.saturating_sub(cmd.name().len());
            for _ in 0..padding {
                uart_write_all(uart, b" ").await;
            }

            uart_write_all(uart, b" - ").await;
            uart_write_all(uart, cmd.description().as_bytes()).await;
            uart_write_all(uart, b"\r\n").await;
        }
    }
}

pub struct LedCommand;
impl Command for LedCommand {
    fn name(&self) -> &str {
        "led"
    }
    fn description(&self) -> &str {
        "Control GP28 LED (led on/off)"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        if args.is_empty() {
            uart_write_all(uart, b"Usage: led <on|off>\r\n").await;
            return;
        }

        match args[0] {
            "on" => {
                led.set_high();
                uart_write_all(uart, b"LED is ON\r\n").await;
            }
            "off" => {
                led.set_low();
                uart_write_all(uart, b"LED is OFF\r\n").await;
            }
            _ => {
                uart_write_all(uart, b"Unknown LED state. Use 'on' or 'off'.\r\n").await;
            }
        }
    }
}

pub struct InfoCommand;
impl Command for InfoCommand {
    fn name(&self) -> &str {
        "info"
    }
    fn description(&self) -> &str {
        "Show system info"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        _args: &[&str],
        uid_str: &str,
    ) {
        uart_write_all(uart, b"System: Raspberry Pi Pico 2 W\r\n").await;
        uart_write_all(uart, b"CPU: RP2350 (RISC-V/ARM)\r\n").await;
        uart_write_all(uart, b"WiFi/BLE: CYW43439\r\n").await;
        uart_write_all(uart, b"UID: ").await;
        uart_write_all(uart, uid_str.as_bytes()).await;
        uart_write_all(uart, b"\r\n").await;
    }
}

pub struct EchoCommand;
impl Command for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Echo the input message"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        for (i, arg) in args.iter().enumerate() {
            uart_write_all(uart, arg.as_bytes()).await;
            if i < args.len() - 1 {
                uart_write_all(uart, b" ").await;
            }
        }
        uart_write_all(uart, b"\r\n").await;
    }
}

pub struct LogCommand;
impl Command for LogCommand {
    fn name(&self) -> &str {
        "log"
    }
    fn description(&self) -> &str {
        "Manage logs (print, clear, record) and level (error, warn, info, debug, trace)"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        use crate::log_filter::LOG_LEVEL;
        use core::sync::atomic::Ordering;

        if args.is_empty() {
            let level = LOG_LEVEL.load(Ordering::Relaxed);
            let level_str = match level {
                0 => "error",
                1 => "warn",
                2 => "info",
                3 => "debug",
                4 => "trace",
                _ => "unknown",
            };
            let _ = uart_write_all(uart, b"Current log level: ").await;
            let _ = uart_write_all(uart, level_str.as_bytes()).await;
            let _ = uart_write_all(uart, b"\r\n").await;
            return;
        }

        let new_level = match args[0] {
            "print" => {
                let _ = crate::logger::log_print(uart).await;
                return;
            }
            "clear" => {
                let _ = crate::logger::log_clear().await;
                let _ = uart_write_all(uart, b"Log cleared.\r\n").await;
                return;
            }
            "record" => {
                if args.len() > 1 {
                    let mut msg: heapless::String<256> = heapless::String::new();
                    for (i, arg) in args[1..].iter().enumerate() {
                        if i > 0 {
                            let _ = msg.push_str(" ");
                        }
                        let _ = msg.push_str(arg);
                    }
                    let _ = crate::logger::write_log(msg.as_str()).await;
                    let _ = uart_write_all(uart, b"Log recorded.\r\n").await;
                } else {
                    let _ = uart_write_all(uart, b"Usage: log record <message>\r\n").await;
                }
                return;
            }
            "error" => 0,
            "warn" => 1,
            "info" => 2,
            "debug" => 3,
            "trace" => 4,
            _ => {
                let _ = uart_write_all(
                    uart,
                    b"Invalid use. Subcommands: print, clear, record. Levels: error, warn, info, debug, trace\r\n",
                )
                .await;
                return;
            }
        };

        LOG_LEVEL.store(new_level, Ordering::Relaxed);
        let _ = uart_write_all(uart, b"Log level set to ").await;
        let _ = uart_write_all(uart, args[0].as_bytes()).await;
        let _ = uart_write_all(uart, b"\r\n").await;
    }
}

pub struct RebootCommand;
impl Command for RebootCommand {
    fn name(&self) -> &str {
        "reboot"
    }
    fn description(&self) -> &str {
        "Reset the system"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        _args: &[&str],
        _uid_str: &str,
    ) {
        uart_write_all(uart, b"Rebooting to bootloader...\r\n").await;
        // Wait a bit for the message to be sent
        embassy_time::Timer::after_millis(100).await;
        cortex_m::peripheral::SCB::sys_reset();
    }
}

pub struct AuthCommand;
impl Command for AuthCommand {
    fn name(&self) -> &str {
        "auth"
    }
    fn description(&self) -> &str {
        "Authenticate the BLE connection"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        uid_str: &str,
    ) {
        if args.is_empty() {
            uart_write_all(uart, b"Usage: auth <passkey>\r\n").await;
            return;
        }

        // Expected passkey is the last 6 characters of the UID (XXXXXX)
        let passkey = if uid_str.len() >= 6 {
            &uid_str[uid_str.len() - 6..]
        } else {
            uid_str
        };

        if args[0] == passkey {
            BLE_AUTHENTICATED.store(true, Ordering::SeqCst);
            uart_write_all(uart, b"Authentication successful. Shell unlocked.\r\n").await;
        } else {
            uart_write_all(uart, b"Authentication failed. Incorrect passkey.\r\n").await;
        }
    }
}

pub struct MkdirCommand;
impl Command for MkdirCommand {
    fn name(&self) -> &str {
        "mkdir"
    }
    fn description(&self) -> &str {
        "Create a directory in the filesystem"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        if args.is_empty() {
            uart_write_all(uart, b"Usage: mkdir <path>\r\n").await;
            return;
        }
        if crate::logger::fs_mkdir(args[0]).await.is_err() {
            uart_write_all(uart, b"error: failed to create directory\r\n").await;
        } else {
            uart_write_all(uart, b"OK\r\n").await;
        }
    }
}

pub struct CdCommand;
impl Command for CdCommand {
    fn name(&self) -> &str {
        "cd"
    }
    fn description(&self) -> &str {
        "Change the current working directory"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        if args.is_empty() {
            uart_write_all(uart, b"Usage: cd <path>\r\n").await;
            return;
        }
        if crate::logger::fs_cd(args[0]).await.is_err() {
            uart_write_all(uart, b"error: no such file or directory\r\n").await;
        }
    }
}

pub struct LsCommand;
impl Command for LsCommand {
    fn name(&self) -> &str {
        "ls"
    }
    fn description(&self) -> &str {
        "List directory contents"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        let path = if args.is_empty() { None } else { Some(args[0]) };
        if crate::logger::fs_ls(uart, path).await.is_err() {
            uart_write_all(uart, b"error: failed to list directory\r\n").await;
        }
    }
}

pub struct CatCommand;
impl Command for CatCommand {
    fn name(&self) -> &str {
        "cat"
    }
    fn description(&self) -> &str {
        "Read file content and stream to terminal"
    }
    async fn exec(
        &self,
        uart: &mut UartTx<'static, Async>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
    ) {
        if args.is_empty() {
            uart_write_all(uart, b"Usage: cat <path>\r\n").await;
            return;
        }
        let _ = crate::logger::fs_cat(uart, args[0]).await;
    }
}

pub async fn handle_command(
    line: &str,
    uart: &mut UartTx<'static, Async>,
    led: &mut Output<'static>,
    uid_str: &str,
    from_ble: bool,
) {
    let mut parts = line.split_whitespace();
    let Some(cmd_name) = parts.next() else { return };
    let args_vec: heapless::Vec<&str, 8> = parts.collect();
    let args = &args_vec;

    // 1. COMMANDS 배열에서 일치 하는 커맨트 찾기
    let target = COMMANDS.iter().find(|c| c.name() == cmd_name);

    match target {
        Some(cmd) => {
            // 2. 인증 체크 (메서드 활용)
            if from_ble && !BLE_AUTHENTICATED.load(Ordering::SeqCst) {
                if cmd_name != AuthCommand.name() && cmd_name != RebootCommand.name() {
                    uart_write_all(uart, b"Unauthored. Please run 'auth <passkey>' first.\r\n")
                        .await;
                    return;
                }
            }
            // 3. 실행
            cmd.exec(uart, led, args, uid_str).await;
        }
        None => {
            // 4. 알 수 없는 명령어 처리
            uart_write_all(uart, b"Unknown command: ").await;
            uart_write_all(uart, cmd_name.as_bytes()).await;
            uart_write_all(uart, b"\r\n").await;
        }
    }
}
