//! pico2w-shell-rs using Embassy
//! Background blinking for CYW43 LED and Async UART CLI

#![no_std]
#![no_main]

mod cli;
mod dhcp;
mod log_filter;
mod logger;

use cyw43_pio::PioSpi;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, DMA_CH2, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::uart::{Blocking, Config, Uart};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

use embassy_net::tcp::TcpSocket;
use embassy_net::{Stack, StackResources};
use heapless::Vec;

#[cfg(target_arch = "riscv32")]
use panic_halt as _;
#[cfg(target_arch = "arm")]
use panic_probe as _;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>,
                 embassy_rp::dma::InterruptHandler<DMA_CH1>,
                 embassy_rp::dma::InterruptHandler<DMA_CH2>,
                 embassy_rp::dma::InterruptHandler<embassy_rp::peripherals::DMA_CH3>;
});

/// Background task to blink the CYW43 LED
#[embassy_executor::task]
async fn blink_task(mut control: cyw43::Control<'static>) {
    loop {
        control.gpio_set(0, true).await;
        Timer::after(Duration::from_millis(500)).await;
        control.gpio_set(0, false).await;
        Timer::after(Duration::from_millis(500)).await;
    }
}

pub static TCP_RX_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    Vec<u8, 64>,
    8,
> = embassy_sync::channel::Channel::new();

pub static TCP_TX_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    Vec<u8, 64>,
    16,
> = embassy_sync::channel::Channel::new();

/// Task to handle UART CLI
#[embassy_executor::task]
async fn uart_task(uart: Uart<'static, Blocking>, mut led: Output<'static>, uid_str: &'static str) {
    let (mut tx, mut rx) = uart.split();
    let mut buf = [0u8; 64];
    let mut idx = 0;

    cli::uart_write_all(
        &mut tx,
        b"\r\nPico 2W Shell (Embassy with WiFi TCP)\r\nType 'help' for commands.\r\n> ",
    )
    .await;

    loop {
        let mut uart_byte = None;
        match embedded_hal_nb::serial::Read::read(&mut rx) {
            Ok(b) => uart_byte = Some(b),
            Err(nb::Error::WouldBlock) => { /* empty */ }
            Err(_) => { /* error */ }
        }

        if let Some(c) = uart_byte {
            // Process UART byte
            if c == b'\r' || c == b'\n' {
                cli::uart_write_all(&mut tx, b"\r\n").await;
                if idx > 0 {
                    if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                        cli::handle_command(line, &mut tx, &mut led, uid_str, false).await;
                    }
                    idx = 0;
                }
                cli::uart_write_all(&mut tx, b"> ").await;
            } else if c == 0x08 || c == 0x7F {
                if idx > 0 {
                    idx -= 1;
                    cli::uart_write_all(&mut tx, b"\x08 \x08").await;
                }
            } else if idx < buf.len() {
                cli::uart_write_all(&mut tx, &[c]).await;
                buf[idx] = c;
                idx += 1;
            }
        } else if let Ok(tcp_data) = TCP_RX_CHANNEL.try_receive() {
            // Process TCP Data
            for &c in tcp_data.iter() {
                if c == b'\r' || c == b'\n' {
                    cli::uart_write_all(&mut tx, b"\r\n").await;
                    if idx > 0 {
                        if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                            cli::handle_command(line, &mut tx, &mut led, uid_str, true).await;
                        }
                        idx = 0;
                    }
                    cli::uart_write_all(&mut tx, b"> ").await;
                } else if c == 0x08 || c == 0x7F {
                    if idx > 0 {
                        idx -= 1;
                        cli::uart_write_all(&mut tx, b"\x08 \x08").await;
                    }
                } else if idx < buf.len() {
                    cli::uart_write_all(&mut tx, &[c]).await;
                    buf[idx] = c;
                    idx += 1;
                }
            }
        } else {
            // Yield executor to allow CYW43 and TCP to run
            embassy_time::Timer::after(embassy_time::Duration::from_millis(5)).await;
        }
    }
}

type MyCywBus = cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>;

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, MyCywBus>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn tcp_server_task(stack: Stack<'static>) {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        defmt::info!("Listening on TCP:8080...");
        if let Err(e) = socket.accept(8080).await {
            defmt::warn!("TCP accept error: {:?}", e);
            continue;
        }

        defmt::info!("TCP Client connected!");

        loop {
            let mut read_buf = [0; 64];
            let read_fut = socket.read(&mut read_buf);
            let tx_ch_fut = TCP_TX_CHANNEL.receive();

            match embassy_futures::select::select(read_fut, tx_ch_fut).await {
                embassy_futures::select::Either::First(Ok(n)) => {
                    if n == 0 {
                        defmt::info!("TCP connection closed");
                        break;
                    }
                    let mut vec: Vec<u8, 64> = Vec::new();
                    let _ = vec.extend_from_slice(&read_buf[..n]);
                    let _ = TCP_RX_CHANNEL.try_send(vec);
                }
                embassy_futures::select::Either::First(Err(e)) => {
                    defmt::warn!("TCP Read Error: {:?}", e);
                    break;
                }
                embassy_futures::select::Either::Second(tx_data) => {
                    if let Err(e) = embedded_io_async::Write::write_all(&mut socket, &tx_data).await
                    {
                        defmt::warn!("TCP Write Error: {:?}", e);
                        break;
                    }
                }
            }
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    // Give RTT a moment to connect
    Timer::after(Duration::from_millis(500)).await;

    // Configure UART Blocking early to trace system boot
    let uart_config = Config::default();
    let mut uart = Uart::new_blocking(p.UART0, p.PIN_0, p.PIN_1, uart_config);
    let _ = uart.blocking_write(b"\r\n\r\n=== SYSTEM BOOTING ===\r\n");

    // Read the 64-bit random chip ID from RP2350 OTP rows 0x0-0x3
    let uid_u64 = embassy_rp::otp::get_chipid().unwrap_or(0);
    let uid = uid_u64.to_be_bytes();

    defmt::info!("RP2350 OTP Chip ID: {:a}", uid);

    // Create formatted strings
    static UID_STR: StaticCell<heapless::String<64>> = StaticCell::new();
    let uid_str = UID_STR.init(heapless::String::new());
    core::fmt::write(
        uid_str,
        format_args!(
            "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            uid[0], uid[1], uid[2], uid[3], uid[4], uid[5], uid[6], uid[7]
        ),
    )
    .unwrap();

    static BLE_NAME: StaticCell<heapless::String<64>> = StaticCell::new();
    let ble_name = BLE_NAME.init(heapless::String::new());
    core::fmt::write(
        ble_name,
        format_args!(
            "Pico_2W_Shell_{:02X}{:02X}{:02X}{:02X}",
            uid[4], uid[5], uid[6], uid[7]
        ),
    )
    .unwrap();

    let fw = cyw43::aligned_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = cyw43::aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
    let nvram = cyw43::aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);

    // Initialize Flash for logger
    let _ = uart.blocking_write(b"-> Initializing Logger...\r\n");
    let flash = embassy_rp::flash::Flash::new(p.FLASH, p.DMA_CH2, Irqs);

    if let Err(_e) = logger::init(flash) {
        defmt::error!("Logger init failed!");
    } else {
        defmt::info!("Logger initialized successfully.");
        // Test log
        let _ = embassy_futures::block_on(logger::log_write_all(b"System booted."));
    }

    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        cyw43_pio::DEFAULT_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        embassy_rp::dma::Channel::new(p.DMA_CH0, Irqs),
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    let _ = uart.blocking_write(b"-> New cyw43...\r\n");
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw, nvram).await;

    spawner.spawn(unwrap!(cyw43_task(runner)));

    let _ = uart.blocking_write(b"-> Init cyw43...\r\n");
    control.init(clm).await;

    let _ = uart.blocking_write(b"-> Config Power Save...\r\n");
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let passkey = if uid_str.len() >= 8 {
        &uid_str[uid_str.len() - 8..]
    } else {
        uid_str.as_str()
    };

    let _ = uart.blocking_write(b"-> Configuring Network Stack...\r\n");
    let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: embassy_net::Ipv4Cidr::new(embassy_net::Ipv4Address::new(192, 168, 4, 1), 24),
        gateway: Some(embassy_net::Ipv4Address::new(192, 168, 4, 1)),
        dns_servers: heapless::Vec::new(),
    });

    // Generate MAC address to avoid clash or just give random MAC Seed.
    static STACK: StaticCell<Stack> = StaticCell::new();
    static RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();
    let (stack_inst, net_runner) = embassy_net::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<2>::new()),
        uid_u64,
    );
    let stack = &*STACK.init(stack_inst);

    spawner.spawn(unwrap!(net_task(net_runner)));
    spawner.spawn(unwrap!(tcp_server_task(*stack)));
    spawner.spawn(unwrap!(dhcp::dhcp_server_task(
        *stack,
        embassy_net::Ipv4Address::new(192, 168, 4, 1),
    )));

    // Start SoftAP on Wi-Fi channel 6
    let mut start_msg = heapless::String::<128>::new();
    let _ = core::fmt::write(
        &mut start_msg,
        format_args!(
            "-> Starting SoftAP (SSID: '{}', Passkey: '{}')...\r\n",
            ble_name.as_str(),
            passkey
        ),
    );
    let _ = uart.blocking_write(start_msg.as_bytes());
    let _ = control.start_ap_wpa2(ble_name.as_str(), passkey, 6).await;

    // PIN 28 LED
    let _ = uart.blocking_write(b"-> Spawning Application Tasks...\r\n");
    let led = Output::new(p.PIN_28, Level::Low);

    // Spawn tasks
    spawner.spawn(unwrap!(blink_task(control)));
    spawner.spawn(unwrap!(uart_task(uart, led, uid_str.as_str())));

    // log_info!("Tasks spawned");
    loop {
        Timer::after(Duration::from_millis(1000)).await;
    }
}

/// Program metadata for `picotool info`
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Pico 2W Embassy Shell"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_description!(
        c"Pico 2 W Embassy Shell with background LED and UART CLI"
    ),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];
