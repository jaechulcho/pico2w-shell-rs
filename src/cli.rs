use core::sync::atomic::{AtomicBool, Ordering};
use embassy_rp::gpio::Output;
use embassy_rp::uart::{Async, Uart};
//use embedded_io_async::Write;

use crate::ble;

static BLE_AUTHENTICATED: AtomicBool = AtomicBool::new(false);

pub async fn uart_write_all(uart: &mut Uart<'static, Async>, buf: &[u8]) {
    let _ = uart.write(buf).await;
    // Also send to BLE TX Channel
    if buf.len() > 0 {
        let mut vec = heapless::Vec::new();
        // Break into 64-byte chunks if needed, but for now just send what fits
        let _ = vec.extend_from_slice(&buf[..core::cmp::min(buf.len(), 64)]);
        let _ = ble::BLE_TX_CHANNEL.try_send(vec);
    }
}

pub trait Command {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn exec(
        &self,
        uart: &mut Uart<'static, Async>,
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
        uart: &mut Uart<'static, Async>,
        _led: &mut Output<'static>,
        _args: &[&str],
        _uid_str: &str,
    ) {
        uart_write_all(uart, b"Available commands:\r\n").await;
        uart_write_all(uart, b"  help    - Show this help\r\n").await;
        uart_write_all(uart, b"  led <on|off> - Control the GP28 LED\r\n").await;
        uart_write_all(uart, b"  info    - Show system info\r\n").await;
        uart_write_all(uart, b"  echo <msg>    - Echo the message\r\n").await;
        uart_write_all(uart, b"  reboot  - Reset the system to bootloader\r\n").await;
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
        uart: &mut Uart<'static, Async>,
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
        uart: &mut Uart<'static, Async>,
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
        uart: &mut Uart<'static, Async>,
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
        "Set log level (error, warn, info, debug, trace)"
    }
    async fn exec(
        &self,
        uart: &mut Uart<'static, Async>,
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
            "error" => 0,
            "warn" => 1,
            "info" => 2,
            "debug" => 3,
            "trace" => 4,
            _ => {
                let _ = uart_write_all(
                    uart,
                    b"Invalid level. Use: error, warn, info, debug, trace\r\n",
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
        uart: &mut Uart<'static, Async>,
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
        uart: &mut Uart<'static, Async>,
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

pub async fn handle_command(
    line: &str,
    uart: &mut Uart<'static, Async>,
    led: &mut Output<'static>,
    uid_str: &str,
    from_ble: bool,
) {
    let mut parts = line.split_whitespace();
    if let Some(cmd_name) = parts.next() {
        let args_vec: heapless::Vec<&str, 8> = parts.collect();
        let args = &args_vec;

        // Check if authentication is required
        if from_ble && !BLE_AUTHENTICATED.load(Ordering::SeqCst) {
            // Unlocked commands
            if cmd_name != "auth" && cmd_name != "reboot" {
                uart_write_all(
                    uart,
                    b"Unauthorized. Please run 'auth <passkey>' first.\r\n",
                )
                .await;
                return;
            }
        }

        match cmd_name {
            "help" => HelpCommand.exec(uart, led, args, uid_str).await,
            "led" => LedCommand.exec(uart, led, args, uid_str).await,
            "info" => InfoCommand.exec(uart, led, args, uid_str).await,
            "echo" => EchoCommand.exec(uart, led, args, uid_str).await,
            "log" => LogCommand.exec(uart, led, args, uid_str).await,
            "auth" => AuthCommand.exec(uart, led, args, uid_str).await,
            "reboot" => RebootCommand.exec(uart, led, args, uid_str).await,
            _ => {
                uart_write_all(uart, b"Unknown command: ").await;
                uart_write_all(uart, cmd_name.as_bytes()).await;
                uart_write_all(uart, b"\r\n").await;
            }
        }
    }
}
