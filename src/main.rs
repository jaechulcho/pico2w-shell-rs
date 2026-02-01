//! pico2w-shell-rs using Embassy
//! Background blinking for CYW43 LED and Async UART CLI

#![no_std]
#![no_main]

mod cli;
mod log_filter;

use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
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

// Use trait for write_all
//use embedded_io_async::Write;

#[cfg(target_arch = "riscv32")]
use panic_halt as _;
#[cfg(target_arch = "arm")]
use panic_probe as _;

use crate::log_filter::LOG_LEVEL;
use core::sync::atomic::Ordering;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    UART0_IRQ => UartInterruptHandler<UART0>;
    DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>,
                 embassy_rp::dma::InterruptHandler<DMA_CH1>,
                 embassy_rp::dma::InterruptHandler<DMA_CH2>;
});

/// Background task to blink the CYW43 LED
#[embassy_executor::task]
async fn blink_task(mut control: cyw43::Control<'static>) {
    loop {
        log_info!("Blink on");
        control.gpio_set(0, true).await;
        Timer::after(Duration::from_millis(500)).await;
        log_info!("Blink off");
        control.gpio_set(0, false).await;
        Timer::after(Duration::from_millis(500)).await;
    }
}

/// Helper to write all bytes to UART
async fn uart_write_all(uart: &mut Uart<'static, Async>, buf: &[u8]) {
    let _ = uart.write(buf).await;
}

/// Task to handle UART CLI
#[embassy_executor::task]
async fn uart_task(mut uart: Uart<'static, Async>, mut led: Output<'static>) {
    let mut buf = [0u8; 64];
    let mut idx = 0;

    uart_write_all(
        &mut uart,
        b"\r\nPico 2W Shell (Embassy)\r\nType 'help' for commands.\r\n> ",
    )
    .await;

    loop {
        let mut byte = [0u8; 1];
        if let Ok(_) = uart.read(&mut byte).await {
            let c = byte[0];

            if c == b'\r' || c == b'\n' {
                uart_write_all(&mut uart, b"\r\n").await;
                if idx > 0 {
                    if let Ok(line) = core::str::from_utf8(&buf[..idx]) {
                        cli::handle_command(line, &mut uart, &mut led).await;
                    }
                    idx = 0;
                }
                uart_write_all(&mut uart, b"> ").await;
            } else if c == 0x08 || c == 0x7F {
                if idx > 0 {
                    idx -= 1;
                    uart_write_all(&mut uart, b"\x08 \x08").await;
                }
            } else if idx < buf.len() {
                // Echo standard characters
                uart_write_all(&mut uart, &[c]).await;
                buf[idx] = c;
                idx += 1;
            }
        }
    }
}

type MyCywBus = cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>;

#[embassy_executor::task]
async fn cyw43_task(runner: cyw43::Runner<'static, MyCywBus>) -> ! {
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    log_info!("Pico 2 W Embassy Start");
    LOG_LEVEL.store(1, Ordering::Relaxed);

    // Include firmware
    // Note: Missing files in cyw43-firmware/ will cause build error.
    let fw = cyw43::aligned_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = cyw43::aligned_bytes!("../cyw43-firmware/43439A0_clm.bin");
    let nvram = cyw43::aligned_bytes!("../cyw43-firmware/nvram_rp2040.bin");

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);

    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        RM2_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        embassy_rp::dma::Channel::new(p.DMA_CH0, Irqs),
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    let (_net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw, nvram).await;
    // Task spawning
    spawner.spawn(unwrap!(cyw43_task(runner)));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // Configure UART
    let uart_config = Config::default();
    let uart = Uart::new(
        p.UART0,
        p.PIN_0,
        p.PIN_1,
        Irqs,
        p.DMA_CH1,
        p.DMA_CH2,
        uart_config,
    );

    // PIN 28 LED
    let led = Output::new(p.PIN_28, Level::Low);

    // Spawn tasks
    spawner.spawn(unwrap!(blink_task(control)));
    spawner.spawn(unwrap!(uart_task(uart, led)));

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
