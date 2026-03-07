use core::sync::atomic::{AtomicBool, Ordering};
use embassy_net::Stack;
use embassy_rp::gpio::Output;
use embassy_rp::uart::{Blocking, UartTx};
use embedded_hal_nb::serial::Write;
use heapless::Vec;

// 모든 커맨트 인스턴스를 모아둔 배열
pub enum CliOutput<'a> {
    Uart(&'a mut UartTx<'static, Blocking>),
    Buffer(&'a mut heapless::String<2048>),
    Tcp(
        &'a mut embassy_sync::channel::Sender<
            'static,
            embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
            Vec<u8, 64>,
            16,
        >,
    ),
}

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
    WifiReset(WifiResetCommand),
    SysScan(SysScanCommand),
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
            Self::WifiReset(c) => c.name(),
            Self::SysScan(c) => c.name(),
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
            Self::WifiReset(c) => c.description(),
            Self::SysScan(c) => c.description(),
        }
    }

    async fn exec(
        &self,
        out: &mut CliOutput<'_>,
        led: &mut Output<'static>,
        args: &[&str],
        uid_str: &str,
        stack: Stack<'static>,
    ) {
        match self {
            Self::Help(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Led(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Info(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Echo(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Log(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Auth(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Reboot(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Mkdir(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Cd(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Ls(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::Cat(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::WifiReset(c) => c.exec(out, led, args, uid_str, stack).await,
            Self::SysScan(c) => c.exec(out, led, args, uid_str, stack).await,
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
    CommandEnum::WifiReset(WifiResetCommand),
    CommandEnum::SysScan(SysScanCommand),
];

static TCP_AUTHENTICATED: AtomicBool = AtomicBool::new(false);

pub async fn uart_write_all(out: &mut CliOutput<'_>, buf: &[u8], _stack: Stack<'static>) {
    if buf.is_empty() {
        return;
    }
    match out {
        CliOutput::Uart(uart) => {
            for &b in buf {
                while let Err(nb::Error::WouldBlock) = uart.write(b) {}
            }
        }
        CliOutput::Buffer(s) => {
            if let Ok(str_buf) = core::str::from_utf8(buf) {
                let _ = s.push_str(str_buf);
            }
        }
        CliOutput::Tcp(tx_ch) => {
            for chunk in buf.chunks(64) {
                let mut vec: heapless::Vec<u8, 64> = heapless::Vec::new();
                if vec.extend_from_slice(chunk).is_ok() {
                    let _ = tx_ch.send(vec).await;
                }
            }
        }
    }
}

pub trait Command {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn exec(
        &self,
        out: &mut CliOutput<'_>,
        led: &mut Output<'static>,
        args: &[&str],
        uid_str: &str,
        stack: Stack<'static>,
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        _args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        uart_write_all(out, b"Available commands:\r\n", stack).await;

        for cmd in COMMANDS {
            // 각 커맨트의 name()과 description()을 재사용
            uart_write_all(out, b"  ", stack).await;
            uart_write_all(out, cmd.name().as_bytes(), stack).await;

            // 간격을 맞추기 위한 공백 처리 (예, 12칸 정렬)
            let padding = 12usize.saturating_sub(cmd.name().len());
            for _ in 0..padding {
                uart_write_all(out, b" ", stack).await;
            }

            uart_write_all(out, b" - ", stack).await;
            uart_write_all(out, cmd.description().as_bytes(), stack).await;
            uart_write_all(out, b"\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        if args.is_empty() {
            uart_write_all(out, b"Usage: led <on|off>\r\n", stack).await;
            return;
        }

        match args[0] {
            "on" => {
                led.set_high();
                uart_write_all(out, b"LED is ON\r\n", stack).await;
            }
            "off" => {
                led.set_low();
                uart_write_all(out, b"LED is OFF\r\n", stack).await;
            }
            _ => {
                uart_write_all(out, b"Unknown LED state. Use 'on' or 'off'.\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        _args: &[&str],
        uid_str: &str,
        stack: Stack<'static>,
    ) {
        uart_write_all(out, b"System: Raspberry Pi Pico 2 W\r\n", stack).await;
        uart_write_all(out, b"CPU: RP2350 (RISC-V/ARM)\r\n", stack).await;
        uart_write_all(out, b"WiFi/BLE: CYW43439\r\n", stack).await;
        uart_write_all(out, b"UID: ", stack).await;
        uart_write_all(out, uid_str.as_bytes(), stack).await;
        uart_write_all(out, b"\r\n", stack).await;

        if let Some(config) = crate::logger::read_wifi_conf().await {
            uart_write_all(out, b"Mode: Station (Connected to ", stack).await;
            uart_write_all(out, config.ssid.as_bytes(), stack).await;
            uart_write_all(out, b")\r\n", stack).await;
        } else {
            uart_write_all(out, b"Mode: Setup SoftAP (AP Mode)\r\n", stack).await;
        }

        if let Some(config) = stack.config_v4() {
            let addr = config.address.address();
            let mut ip_str = heapless::String::<32>::new();
            let octets = addr.octets();
            let _ = core::fmt::write(
                &mut ip_str,
                format_args!(
                    "IP: {}.{}.{}.{}\r\n",
                    octets[0], octets[1], octets[2], octets[3]
                ),
            );
            uart_write_all(out, ip_str.as_bytes(), stack).await;
        } else {
            uart_write_all(out, b"IP: Not acquired\r\n", stack).await;
        }
        uart_write_all(out, b"Hostname: pico.local\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        for (i, arg) in args.iter().enumerate() {
            uart_write_all(out, arg.as_bytes(), stack).await;
            if i < args.len() - 1 {
                uart_write_all(out, b" ", stack).await;
            }
        }
        uart_write_all(out, b"\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
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
            let _ = uart_write_all(out, b"Current log level: ", stack).await;
            let _ = uart_write_all(out, level_str.as_bytes(), stack).await;
            let _ = uart_write_all(out, b"\r\n", stack).await;
            return;
        }

        let new_level = match args[0] {
            "print" => {
                let _ = crate::logger::log_print(out, stack).await;
                return;
            }
            "clear" => {
                let _ = crate::logger::log_clear().await;
                let _ = uart_write_all(out, b"Log cleared.\r\n", stack).await;
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
                    let _ = uart_write_all(out, b"Log recorded.\r\n", stack).await;
                } else {
                    let _ = uart_write_all(out, b"Usage: log record <message>\r\n", stack).await;
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
                    out,
                    b"Invalid use. Subcommands: print, clear, record. Levels: error, warn, info, debug, trace\r\n",
                    stack,
                )
                .await;
                return;
            }
        };

        LOG_LEVEL.store(new_level, Ordering::Relaxed);
        let _ = uart_write_all(out, b"Log level set to ", stack).await;
        let _ = uart_write_all(out, args[0].as_bytes(), stack).await;
        let _ = uart_write_all(out, b"\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        _args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        uart_write_all(out, b"Rebooting to bootloader...\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        uid_str: &str,
        stack: Stack<'static>,
    ) {
        if args.is_empty() {
            uart_write_all(out, b"Usage: auth <passkey>\r\n", stack).await;
            return;
        }

        // Expected passkey is the last 6 characters of the UID (XXXXXX)
        let passkey = if uid_str.len() >= 6 {
            &uid_str[uid_str.len() - 6..]
        } else {
            uid_str
        };

        if args[0] == passkey {
            TCP_AUTHENTICATED.store(true, Ordering::SeqCst);
            uart_write_all(
                out,
                b"Authentication successful. Shell unlocked.\r\n",
                stack,
            )
            .await;
        } else {
            uart_write_all(out, b"Authentication failed. Incorrect passkey.\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        if args.is_empty() {
            uart_write_all(out, b"Usage: mkdir <path>\r\n", stack).await;
            return;
        }
        if crate::logger::fs_mkdir(args[0]).await.is_err() {
            uart_write_all(out, b"error: failed to create directory\r\n", stack).await;
        } else {
            uart_write_all(out, b"OK\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        if args.is_empty() {
            uart_write_all(out, b"Usage: cd <path>\r\n", stack).await;
            return;
        }
        if crate::logger::fs_cd(args[0]).await.is_err() {
            uart_write_all(out, b"error: no such file or directory\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        let path = if args.is_empty() { None } else { Some(args[0]) };
        if crate::logger::fs_ls(out, path, stack).await.is_err() {
            uart_write_all(out, b"error: failed to list directory\r\n", stack).await;
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
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        if args.is_empty() {
            uart_write_all(out, b"Usage: cat <path>\r\n", stack).await;
            return;
        }
        let _ = crate::logger::fs_cat(out, args[0], stack).await;
    }
}

pub async fn handle_command(
    line: &str,
    out: &mut CliOutput<'_>,
    led: &mut Output<'static>,
    uid_str: &str,
    from_tcp: bool,
    stack: Stack<'static>,
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
            if from_tcp
                && !TCP_AUTHENTICATED.load(Ordering::SeqCst)
                && cmd_name != AuthCommand.name()
                && cmd_name != RebootCommand.name()
            {
                uart_write_all(
                    out,
                    b"Unauthored. Please run 'auth <passkey>' first.\r\n",
                    stack,
                )
                .await;
                return;
            }
            // 3. 실행
            cmd.exec(out, led, args, uid_str, stack).await;
        }
        None => {
            // 4. 알 수 없는 명령어 처리
            uart_write_all(out, b"Unknown command: ", stack).await;
            uart_write_all(out, cmd_name.as_bytes(), stack).await;
            uart_write_all(out, b"\r\n", stack).await;
        }
    }
}

pub struct WifiResetCommand;
impl Command for WifiResetCommand {
    fn name(&self) -> &str {
        "wifi"
    }
    fn description(&self) -> &str {
        "Manage WiFi settings (e.g. 'wifi reset')"
    }
    async fn exec(
        &self,
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        if args.is_empty() || args[0] != "reset" {
            uart_write_all(out, b"Usage: wifi reset\r\n", stack).await;
            return;
        }

        uart_write_all(
            out,
            b"Deleting Wi-Fi configuration and rebooting to Setup Mode...\r\n",
            stack,
        )
        .await;
        let _ = crate::logger::delete_wifi_conf().await;
        embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;
        cortex_m::peripheral::SCB::sys_reset();
    }
}

pub struct SysScanCommand;
impl Command for SysScanCommand {
    fn name(&self) -> &str {
        "sys_scan"
    }
    fn description(&self) -> &str {
        "Internal system scan command"
    }
    async fn exec(
        &self,
        out: &mut CliOutput<'_>,
        _led: &mut Output<'static>,
        _args: &[&str],
        _uid_str: &str,
        stack: Stack<'static>,
    ) {
        // Drop stale results
        while let Ok(_) = crate::WIFI_SCAN_RESP_CHANNEL.try_receive() {}

        // Send scan trigger to main task
        let _ = crate::WIFI_SCAN_REQ_CHANNEL.send(()).await;

        // Wait for JSON result
        let result = crate::WIFI_SCAN_RESP_CHANNEL.receive().await;
        uart_write_all(out, result.as_bytes(), stack).await;
    }
}
