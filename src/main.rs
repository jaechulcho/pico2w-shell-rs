//! pico2w-shell-rs using Embassy
//! Background blinking for CYW43 LED and Async UART CLI

#![no_std]
#![no_main]

mod ble;
mod cli;
mod log_filter;

use cyw43_pio::PioSpi;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, DMA_CH2, PIO0, UART0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::uart::{Async, Config, InterruptHandler as UartInterruptHandler, Uart};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;
use trouble_host::prelude::ExternalController;

#[cfg(target_arch = "riscv32")]
use panic_halt as _;
#[cfg(target_arch = "arm")]
use panic_probe as _;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    UART0_IRQ => UartInterruptHandler<UART0>;
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

/// Task to handle UART CLI
#[embassy_executor::task]
async fn uart_task(
    mut uart: Uart<'static, Async>,
    mut led: Output<'static>,
    uid_str: &'static str,
) {
    let mut buf = [0u8; 64];
    let mut idx = 0;

    cli::uart_write_all(
        &mut uart,
        b"\r\nPico 2W Shell (Embassy with BLE)\r\nType 'help' for commands.\r\n> ",
    )
    .await;

    loop {
        let mut byte = [0u8; 1];

        // Select between UART read and BLE RX Channel
        let c_opt =
            embassy_futures::select::select(uart.read(&mut byte), ble::BLE_RX_CHANNEL.receive())
                .await;

        match c_opt {
            embassy_futures::select::Either::First(result) => match result {
                Ok(_) => {
                    let c = byte[0];
                    if c == b'\r' || c == b'\n' {
                        cli::uart_write_all(&mut uart, b"\r\n").await;
                        if idx > 0 {
                            if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                                cli::handle_command(line, &mut uart, &mut led, uid_str).await;
                            }
                            idx = 0;
                        }
                        cli::uart_write_all(&mut uart, b"> ").await;
                    } else if c == 0x08 || c == 0x7F {
                        if idx > 0 {
                            idx -= 1;
                            cli::uart_write_all(&mut uart, b"\x08 \x08").await;
                        }
                    } else if idx < buf.len() {
                        cli::uart_write_all(&mut uart, &[c]).await;
                        buf[idx] = c;
                        idx += 1;
                    }
                }
                Err(e) => {
                    defmt::error!("UART Read Error: {:?}", defmt::Debug2Format(&e));
                    embassy_time::Timer::after(embassy_time::Duration::from_millis(10)).await;
                }
            },
            embassy_futures::select::Either::Second(ble_data) => {
                // Command received from BLE, process it line by line
                for &c in ble_data.iter() {
                    if c == b'\r' || c == b'\n' {
                        cli::uart_write_all(&mut uart, b"\r\n").await;
                        if idx > 0 {
                            if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                                cli::handle_command(line, &mut uart, &mut led, uid_str).await;
                            }
                            idx = 0;
                        }
                        cli::uart_write_all(&mut uart, b"> ").await;
                    } else if c == 0x08 || c == 0x7F {
                        if idx > 0 {
                            idx -= 1;
                            cli::uart_write_all(&mut uart, b"\x08 \x08").await;
                        }
                    } else if idx < buf.len() {
                        cli::uart_write_all(&mut uart, &[c]).await;
                        buf[idx] = c;
                        idx += 1;
                    }
                }
            }
        }
    }
}

type MyCywBus = cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>;

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, MyCywBus>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn ble_host_task(bt_device: cyw43::bluetooth::BtDriver<'static>, device_name: &'static str) {
    let controller: ExternalController<_, 10> = ExternalController::new(bt_device);
    ble::run_ble(controller, device_name).await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    // Give RTT a moment to connect
    Timer::after(Duration::from_millis(500)).await;
    defmt::info!("Pico 2 W Embassy Start (defmt)");

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
        format_args!("Pico 2W Shell {:02X}{:02X}{:02X}", uid[5], uid[6], uid[7]),
    )
    .unwrap();

    let fw = cyw43::aligned_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = cyw43::aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
    let btfw = cyw43::aligned_bytes!("../cyw43-firmware/43439A0_btfw.bin");
    let nvram = cyw43::aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);

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

    let (_net_device, bt_device, mut control, runner) =
        cyw43::new_with_bluetooth(state, pwr, spi, fw, btfw, nvram).await;

    spawner.spawn(unwrap!(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    spawner.spawn(unwrap!(ble_host_task(bt_device, ble_name.as_str())));

    // Configure UART
    let uart_config = Config::default();
    let uart = Uart::new(
        p.UART0,
        p.PIN_0,
        p.PIN_1,
        Irqs,
        p.DMA_CH1,
        p.DMA_CH3, // Instead of CH2 which Flash consumed
        uart_config,
    );

    // PIN 28 LED
    let led = Output::new(p.PIN_28, Level::Low);

    // Spawn tasks
    spawner.spawn(unwrap!(blink_task(control)));
    spawner.spawn(unwrap!(uart_task(uart, led, uid_str.as_str())));

    log_info!("Tasks spawned");
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
