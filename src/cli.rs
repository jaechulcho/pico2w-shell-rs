use embassy_rp::gpio::Output;
use embassy_rp::uart::{Async, Uart};
//use embedded_io_async::Write;

pub trait Command {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn exec(&self, uart: &mut Uart<'static, Async>, led: &mut Output<'static>, args: &[&str]);
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
    ) {
        let _ = uart.write(b"Available commands:\r\n").await;
        let _ = uart.write(b"  help    - Show this help\r\n").await;
        let _ = uart
            .write(b"  led <on|off> - Control the GP28 LED\r\n")
            .await;
        let _ = uart.write(b"  info    - Show system info\r\n").await;
        let _ = uart.write(b"  echo <msg>    - Echo the message\r\n").await;
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
    ) {
        if args.is_empty() {
            let _ = uart.write(b"Usage: led <on|off>\r\n").await;
            return;
        }

        match args[0] {
            "on" => {
                led.set_high();
                let _ = uart.write(b"LED is ON\r\n").await;
            }
            "off" => {
                led.set_low();
                let _ = uart.write(b"LED is OFF\r\n").await;
            }
            _ => {
                let _ = uart
                    .write(b"Unknown LED state. Use 'on' or 'off'.\r\n")
                    .await;
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
    ) {
        let _ = uart
            .write(b"System: Raspberry Pi Pico 2 W (Embassy)\r\n")
            .await;
        let _ = uart.write(b"Chip: RP2350\r\n").await;
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
    ) {
        for (i, arg) in args.iter().enumerate() {
            let _ = uart.write(arg.as_bytes()).await;
            if i < args.len() - 1 {
                let _ = uart.write(b" ").await;
            }
        }
        let _ = uart.write(b"\r\n").await;
    }
}

pub async fn handle_command(
    line: &str,
    uart: &mut Uart<'static, Async>,
    led: &mut Output<'static>,
) {
    let mut parts = line.split_whitespace();
    if let Some(cmd_name) = parts.next() {
        let args_vec: heapless::Vec<&str, 8> = parts.collect();
        let args = &args_vec;

        match cmd_name {
            "help" => HelpCommand.exec(uart, led, args).await,
            "led" => LedCommand.exec(uart, led, args).await,
            "info" => InfoCommand.exec(uart, led, args).await,
            "echo" => EchoCommand.exec(uart, led, args).await,
            _ => {
                let _ = uart.write(b"Unknown command: ").await;
                let _ = uart.write(cmd_name.as_bytes()).await;
                let _ = uart.write(b"\r\n").await;
            }
        }
    }
}
