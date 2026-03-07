//! pico2w-shell-rs using Embassy
//! Background blinking for CYW43 LED and Async UART CLI

#![no_std]
#![no_main]

mod cli;
mod dhcp;
mod http_server;
mod log_filter;
mod logger;
mod ntp;

use cyw43_pio::PioSpi;
use defmt::unwrap;
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
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{HardwareAddress, Ipv4Address, Ipv4Cidr, Stack, StackResources, StaticConfigV4};
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
    POWMAN_IRQ_TIMER => embassy_rp::aon_timer::InterruptHandler;
});

/// Background task to blink the CYW43 LED
#[embassy_executor::task]
async fn blink_task(mut control: cyw43::Control<'static>) {
    defmt::info!("blink_task started");
    loop {
        control.gpio_set(0, true).await;

        // Wait 500ms, checking for scan requests
        match embassy_time::with_timeout(
            Duration::from_millis(500),
            crate::WIFI_SCAN_REQ_CHANNEL.receive(),
        )
        .await
        {
            Ok(_) => {
                defmt::info!("Starting Wi-Fi Scan...");
                let mut scanner = control.scan(cyw43::ScanOptions::default()).await;
                let mut json = heapless::String::<2048>::new();
                let _ = json.push_str("[");
                let mut first = true;

                while let Some(bss) = scanner.next().await {
                    let ssid = core::str::from_utf8(&bss.ssid[..bss.ssid_len as usize])
                        .unwrap_or("Unknown");
                    // Filter empty SSIDs
                    if ssid.trim().is_empty() {
                        continue;
                    }

                    if !first {
                        let _ = json.push_str(",");
                    }
                    first = false;

                    // JSON format: {"ssid":"...","rssi":...}
                    let mut obj = heapless::String::<128>::new();
                    let _ = core::fmt::write(
                        &mut obj,
                        format_args!("{{\"ssid\":\"{}\",\"rssi\":{}}}", ssid, bss.rssi),
                    );
                    if json.len() + obj.len() < json.capacity() - 2 {
                        let _ = json.push_str(obj.as_str());
                    }
                }
                let _ = json.push_str("]");
                defmt::info!("Scan complete!");
                for chunk in json.as_bytes().chunks(64) {
                    let mut vec = heapless::Vec::<u8, 64>::new();
                    let _ = vec.extend_from_slice(chunk);
                    let _ = crate::WIFI_SCAN_RESP_CHANNEL
                        .send(WebResponse::Chunk(vec))
                        .await;
                }
                let _ = crate::WIFI_SCAN_RESP_CHANNEL.send(WebResponse::Done).await;
            }
            Err(_) => {} // Timeout
        }

        control.gpio_set(0, false).await;

        // Wait another 500ms, checking for scan requests again
        match embassy_time::with_timeout(
            Duration::from_millis(500),
            crate::WIFI_SCAN_REQ_CHANNEL.receive(),
        )
        .await
        {
            Ok(_) => {
                defmt::info!("Starting Wi-Fi Scan...");
                let mut scanner = control.scan(cyw43::ScanOptions::default()).await;
                let mut json = heapless::String::<2048>::new();
                let _ = json.push_str("[");
                let mut first = true;

                while let Some(bss) = scanner.next().await {
                    let ssid = core::str::from_utf8(&bss.ssid[..bss.ssid_len as usize])
                        .unwrap_or("Unknown");
                    if ssid.trim().is_empty() {
                        continue;
                    }

                    if !first {
                        let _ = json.push_str(",");
                    }
                    first = false;

                    let mut obj = heapless::String::<128>::new();
                    let _ = core::fmt::write(
                        &mut obj,
                        format_args!("{{\"ssid\":\"{}\",\"rssi\":{}}}", ssid, bss.rssi),
                    );
                    if json.len() + obj.len() < json.capacity() - 2 {
                        let _ = json.push_str(obj.as_str());
                    }
                }
                let _ = json.push_str("]");
                defmt::info!("Scan complete!");
                for chunk in json.as_bytes().chunks(64) {
                    let mut vec = heapless::Vec::<u8, 64>::new();
                    let _ = vec.extend_from_slice(chunk);
                    let _ = crate::WIFI_SCAN_RESP_CHANNEL
                        .send(WebResponse::Chunk(vec))
                        .await;
                }
                let _ = crate::WIFI_SCAN_RESP_CHANNEL.send(WebResponse::Done).await;
            }
            Err(_) => {} // Timeout
        }
    }
}

pub static TCP_RX_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    Vec<u8, 64>,
    16,
> = embassy_sync::channel::Channel::new();

pub static TCP_TX_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    Vec<u8, 64>,
    16,
> = embassy_sync::channel::Channel::new();

pub enum WebResponse {
    Chunk(heapless::Vec<u8, 64>),
    Done,
}

pub struct WebCommand {
    pub cmd: heapless::String<256>,
}

pub static WEB_CMD_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    WebCommand,
    2,
> = embassy_sync::channel::Channel::new();

pub static WEB_RESP_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    WebResponse,
    32, // More slots for streaming
> = embassy_sync::channel::Channel::new();

pub static WIFI_SCAN_REQ_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    (),
    1,
> = embassy_sync::channel::Channel::new();

pub static WIFI_SCAN_RESP_CHANNEL: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::ThreadModeRawMutex,
    WebResponse,
    32,
> = embassy_sync::channel::Channel::new();

/// Task to handle UART CLI
#[embassy_executor::task]
async fn uart_task(
    uart: Uart<'static, Blocking>,
    mut led: Output<'static>,
    uid_str: &'static str,
    stack: Stack<'static>,
) {
    defmt::info!("uart_task started");
    let (mut tx, mut rx) = uart.split();
    let mut buf = [0u8; 64];
    let mut idx = 0;

    cli::uart_write_all(
        &mut cli::CliOutput::Uart(&mut tx),
        b"\r\nPico 2W Shell (Embassy with WiFi TCP)\r\nType 'help' for commands.\r\n> ",
        stack,
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
                cli::uart_write_all(&mut cli::CliOutput::Uart(&mut tx), b"\r\n", stack).await;
                if idx > 0 {
                    if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                        cli::handle_command(
                            line,
                            &mut cli::CliOutput::Uart(&mut tx),
                            &mut led,
                            uid_str,
                            true,
                            stack,
                        )
                        .await;
                    }
                    idx = 0;
                }
                cli::uart_write_all(&mut cli::CliOutput::Uart(&mut tx), b"> ", stack).await;
            } else if c == 0x08 || c == 0x7F {
                if idx > 0 {
                    idx -= 1;
                    cli::uart_write_all(&mut cli::CliOutput::Uart(&mut tx), b"\x08 \x08", stack)
                        .await;
                }
            } else if idx < buf.len() {
                cli::uart_write_all(&mut cli::CliOutput::Uart(&mut tx), &[c], stack).await;
                buf[idx] = c;
                idx += 1;
            }
        } else if let Ok(tcp_data) = TCP_RX_CHANNEL.try_receive() {
            // Process TCP Data
            let mut tcp_out = TCP_TX_CHANNEL.sender();
            for &c in tcp_data.iter() {
                if c == b'\r' || c == b'\n' {
                    cli::uart_write_all(&mut cli::CliOutput::Tcp(&mut tcp_out), b"\r\n", stack)
                        .await;
                    if idx > 0 {
                        if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                            cli::handle_command(
                                line,
                                &mut cli::CliOutput::Tcp(&mut tcp_out),
                                &mut led,
                                uid_str,
                                true,
                                stack,
                            )
                            .await;
                        }
                        idx = 0;
                    }
                    cli::uart_write_all(&mut cli::CliOutput::Tcp(&mut tcp_out), b"> ", stack).await;
                } else if c == 0x08 || c == 0x7F {
                    if idx > 0 {
                        idx -= 1;
                        cli::uart_write_all(
                            &mut cli::CliOutput::Tcp(&mut tcp_out),
                            b"\x08 \x08",
                            stack,
                        )
                        .await;
                    }
                } else if idx < buf.len() {
                    cli::uart_write_all(&mut cli::CliOutput::Tcp(&mut tcp_out), &[c], stack).await;
                    buf[idx] = c;
                    idx += 1;
                }
            }
        } else if let Ok(web_cmd) = WEB_CMD_CHANNEL.try_receive() {
            defmt::info!("Main Loop received web command: {}", web_cmd.cmd.as_str());

            // Clear any stale responses before starting
            while let Ok(_) = WEB_RESP_CHANNEL.try_receive() {}

            let mut web_sender = WEB_RESP_CHANNEL.sender();
            cli::handle_command(
                web_cmd.cmd.as_str(),
                &mut cli::CliOutput::Web(&mut web_sender),
                &mut led,
                uid_str,
                true,
                stack,
            )
            .await;
            defmt::info!("Main Loop command handling done.");
            let _ = WEB_RESP_CHANNEL.send(WebResponse::Done).await;
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

#[embassy_executor::task]
async fn net_config_task(stack: Stack<'static>) {
    loop {
        stack.wait_link_up().await;

        match embassy_time::with_timeout(Duration::from_secs(10), stack.wait_config_up()).await {
            Ok(_) => {
                defmt::info!("Network configuration up: {:?}", stack.config_v4());
            }
            Err(_) => {
                defmt::info!("DHCP timeout (10s), acquiring Link-Local address...");
                let mac = stack.hardware_address();
                let HardwareAddress::Ethernet(eth) = mac;
                let b3 = (eth.0[4] % 254) + 1;
                let b4 = eth.0[5];
                let addr = Ipv4Address::new(169, 254, b3, b4);
                defmt::info!("Assigned Link-Local IP: {}.{}.{}.{}", 169, 254, b3, b4);
                stack.set_config_v4(embassy_net::ConfigV4::Static(StaticConfigV4 {
                    address: Ipv4Cidr::new(addr, 16),
                    gateway: None,
                    dns_servers: Vec::new(),
                }));
            }
        }

        stack.wait_link_down().await;
    }
}

#[embassy_executor::task]
async fn mdns_task(stack: Stack<'static>, short_uid: &'static str) {
    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_payload = [0u8; 512];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_payload = [0u8; 512];

    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_payload,
        &mut tx_meta,
        &mut tx_payload,
    );

    socket.bind(5353).unwrap();

    loop {
        if !stack.has_multicast_group(Ipv4Address::new(224, 0, 0, 251)) {
            let _ = stack.join_multicast_group(Ipv4Address::new(224, 0, 0, 251));
        }

        let mut buf = [0u8; 512];
        let mut query_needle = heapless::String::<32>::new();
        let _ = core::fmt::write(
            &mut query_needle,
            format_args!("\x0bpico-{}\x05local", short_uid),
        );

        match socket.recv_from(&mut buf).await {
            Ok((n, remote)) => {
                let data = &buf[..n];
                // Check for dynamic "pico-XXXXXX" + "local" query
                if data.windows(query_needle.len()).any(|w| {
                    w.iter()
                        .zip(query_needle.as_bytes().iter())
                        .all(|(a, b)| (a | 0x20) == (b | 0x20))
                }) {
                    defmt::info!(
                        "mDNS query for {} received from {:?}",
                        query_needle.as_str(),
                        remote
                    );
                    if let Some(config) = stack.config_v4() {
                        let ip = config.address.address();
                        let ip_bytes = ip.octets();

                        let mut resp = [0u8; 128];
                        resp[0..4].copy_from_slice(&[0x00, 0x00, 0x84, 0x00]); // Answer + Authoritative
                        resp[4..12]
                            .copy_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);

                        let mut pos = 12;
                        // First label: pico-XXXXXX
                        resp[pos] = 11;
                        pos += 1;
                        resp[pos..pos + 5].copy_from_slice(b"pico-");
                        pos += 5;
                        resp[pos..pos + 6].copy_from_slice(short_uid.as_bytes());
                        pos += 6;
                        // Second label: local
                        resp[pos] = 5;
                        pos += 1;
                        resp[pos..pos + 5].copy_from_slice(b"local");
                        pos += 5;
                        resp[pos] = 0;
                        pos += 1;

                        resp[pos..pos + 4].copy_from_slice(&[0x00, 0x01, 0x00, 0x01]); // Type A, Class IN
                        pos += 4;
                        resp[pos..pos + 4].copy_from_slice(&[0x00, 0x00, 0x00, 0x78]); // TTL 120
                        pos += 4;
                        resp[pos..pos + 2].copy_from_slice(&[0x00, 0x04]); // Data length 4
                        pos += 2;
                        resp[pos..pos + 4].copy_from_slice(&ip_bytes);
                        pos += 4;

                        let _ = socket.send_to(&resp[..pos], remote).await;
                        // Also send to multicast group
                        let _ = socket
                            .send_to(
                                &resp[..pos],
                                (
                                    embassy_net::IpAddress::Ipv4(Ipv4Address::new(224, 0, 0, 251)),
                                    5353,
                                ),
                            )
                            .await;
                    }
                }
            }
            Err(e) => {
                defmt::warn!("mDNS Recv Error: {:?}", e);
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

    static SHORT_UID: StaticCell<heapless::String<6>> = StaticCell::new();
    let short_uid = SHORT_UID.init(heapless::String::new());
    let _ = short_uid.push_str(&uid_str[uid_str.len() - 6..]);

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
    // Initialize FileSystem and RTC
    if let Err(e) = logger::init(
        embassy_rp::flash::Flash::new(p.FLASH, p.DMA_CH3, Irqs),
        p.POWMAN,
        Irqs,
    ) {
        defmt::error!("Failed to initialize logger: {:?}", defmt::Debug2Format(&e));
    } else {
        defmt::info!("Logger initialized successfully.");
        let _ = logger::write_log("=== System Boot ===").await;
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
        .set_power_management(cyw43::PowerManagementMode::None)
        .await;

    let mut start_msg = heapless::String::<128>::new();
    let stack_ref = if let Some(config) = crate::logger::read_wifi_conf().await {
        let _ = core::fmt::write(
            &mut start_msg,
            format_args!(
                "-> Starting Station Mode (Connecting to: '{}')...\r\n",
                config.ssid.as_str()
            ),
        );
        let _ = uart.blocking_write(start_msg.as_bytes());

        let config_net = embassy_net::Config::dhcpv4(Default::default());

        static STACK: StaticCell<Stack> = StaticCell::new();
        static RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
        let (stack_inst, net_runner) = embassy_net::new(
            net_device,
            config_net,
            RESOURCES.init(StackResources::<8>::new()),
            uid_u64,
        );
        let stack = &*STACK.init(stack_inst);

        spawner.spawn(unwrap!(net_task(net_runner)));
        spawner.spawn(unwrap!(tcp_server_task(*stack)));
        spawner.spawn(unwrap!(http_server::http_server_task(*stack, true)));
        spawner.spawn(unwrap!(net_config_task(*stack)));
        spawner.spawn(unwrap!(mdns_task(*stack, short_uid.as_str())));
        spawner.spawn(unwrap!(ntp::ntp_sync_task(*stack)));

        let _ = uart.blocking_write(b"-> Joining Wi-Fi AP...\r\n");

        // Try to join AP
        if control
            .join(
                config.ssid.as_str(),
                cyw43::JoinOptions::new(config.pass.as_bytes()),
            )
            .await
            .is_err()
        {
            let _ =
                uart.blocking_write(b"-> Failed to connect. Deleting config and rebooting...\r\n");
            let _ = crate::logger::delete_wifi_conf().await;
            embassy_time::Timer::after(embassy_time::Duration::from_millis(1000)).await;
            cortex_m::peripheral::SCB::sys_reset();
        } else {
            let _ = uart.blocking_write(b"-> WiFi Connected.\r\n");
            let _ = logger::write_log("WiFi Connected (Station Mode)").await;
        }
        stack
    } else {
        // Fallback to Setup SoftAP
        let passkey = if uid_str.len() >= 8 {
            &uid_str[uid_str.len() - 8..]
        } else {
            uid_str.as_str()
        };

        let _ = core::fmt::write(
            &mut start_msg,
            format_args!(
                "-> Starting Setup Mode SoftAP (SSID: '{}', Passkey: '{}')...\r\n",
                ble_name.as_str(),
                passkey
            ),
        );
        let _ = uart.blocking_write(start_msg.as_bytes());

        let config_net = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
            address: embassy_net::Ipv4Cidr::new(embassy_net::Ipv4Address::new(192, 168, 4, 1), 24),
            gateway: Some(embassy_net::Ipv4Address::new(192, 168, 4, 1)),
            dns_servers: heapless::Vec::new(),
        });

        static STACK: StaticCell<Stack> = StaticCell::new();
        static RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
        let (stack_inst, net_runner) = embassy_net::new(
            net_device,
            config_net,
            RESOURCES.init(StackResources::<8>::new()),
            uid_u64,
        );
        let stack = &*STACK.init(stack_inst);

        spawner.spawn(unwrap!(net_task(net_runner)));
        spawner.spawn(unwrap!(tcp_server_task(*stack)));
        spawner.spawn(unwrap!(http_server::http_server_task(*stack, false)));
        spawner.spawn(unwrap!(dhcp::dhcp_server_task(
            *stack,
            embassy_net::Ipv4Address::new(192, 168, 4, 1),
        )));
        spawner.spawn(unwrap!(mdns_task(*stack, short_uid.as_str())));

        let _ = uart.blocking_write(b"-> Starting SoftAP...\r\n");
        let _ = control.start_ap_wpa2(ble_name.as_str(), passkey, 6).await;
        let _ = uart.blocking_write(b"-> SoftAP Started.\r\n");
        stack
    };

    // PIN 28 LED
    let _ = uart.blocking_write(b"-> Spawning Application Tasks...\r\n");
    let led_pin = Output::new(p.PIN_28, Level::Low);

    // Spawn tasks
    spawner.spawn(unwrap!(blink_task(control)));
    spawner.spawn(unwrap!(uart_task(
        uart,
        led_pin,
        uid_str.as_str(),
        *stack_ref
    )));

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
