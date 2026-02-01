//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Copyright (c) 2021–2024 The rp-rs Developers
//! Copyright (c) 2021 rp-rs organization
//! Copyright (c) 2025 Raspberry Pi Ltd.
//!
//! # GPIO 'Blinky' Example
//!
//! This application demonstrates how to control a GPIO pin on the rp2040 and rp235x.
//!
//! It may need to be adapted to your particular board layout and/or pin assignment.

#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::OutputPin;
#[cfg(target_arch = "riscv32")]
use panic_halt as _;
#[cfg(target_arch = "arm")]
use panic_probe as _;

use core::fmt::Write;
use embedded_hal_nb::serial::{Read, Write as _};
use fugit::RateExtU32;
use hal::prelude::*;
use hal::uart::{DataBits, StopBits, UartConfig};

// Alias for our HAL crate
use hal::entry;

#[cfg(rp2350)]
use rp235x_hal as hal;

#[cfg(rp2040)]
use rp2040_hal as hal;

// use bsp::entry;
// use bsp::hal;
// use rp_pico as bsp;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[unsafe(link_section = ".boot2")]
#[used]
#[cfg(rp2040)]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

/// Tell the Boot ROM about our application
#[unsafe(link_section = ".start_block")]
#[used]
#[cfg(rp2350)]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

/// External high-speed crystal on the Raspberry Pi Pico 2 board is 12 MHz.
/// Adjust if your board has a different frequency
const XTAL_FREQ_HZ: u32 = 12_000_000u32;

/// Entry point to our bare-metal application.
///
/// The `#[hal::entry]` macro ensures the Cortex-M start-up code calls this function
/// as soon as all global variables and the spinlock are initialised.
///
/// The function configures the rp2040 and rp235x peripherals, then toggles a GPIO pin in
/// an infinite loop. If there is an LED connected to that pin, it will blink.
#[entry]
fn main() -> ! {
    info!("Program start");
    // Grab our singleton objects
    let mut pac = hal::pac::Peripherals::take().unwrap();

    // Set up the watchdog driver - needed by the clock setup code
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    // Configure the clocks
    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .unwrap();

    // The single-cycle I/O block controls our GPIO pins
    let sio = hal::Sio::new(pac.SIO);

    // Set the pins to their default state
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // Configure UART pins
    let uart_pins = (
        pins.gpio0.into_function::<hal::gpio::FunctionUart>(),
        pins.gpio1.into_function::<hal::gpio::FunctionUart>(),
    );

    // Initialize UART
    let uart = hal::uart::UartPeripheral::new(pac.UART0, uart_pins, &mut pac.RESETS)
        .enable(
            UartConfig::new(115_200.Hz(), DataBits::Eight, None, StopBits::One),
            clocks.peripheral_clock.freq(),
        )
        .unwrap();

    let mut led_pin = pins.gpio28.into_push_pull_output();

    // UART Writer for responses
    let mut uart = uart;

    writeln!(
        uart,
        "\r\nPico 2W Shell (Rust)\r\nType 'help' for commands.\r\n"
    )
    .unwrap();

    let mut input_buf = [0u8; 64];
    let mut input_idx = 0;

    loop {
        // Read from UART (blocking for simplicity)
        if let Ok(c) = nb::block!(uart.read()) {
            // Echo back
            let _ = nb::block!(uart.write(c));

            if c == b'\r' || c == b'\n' {
                let _ = uart.write_str("\r\n");
                if input_idx > 0 {
                    let cmd = core::str::from_utf8(&input_buf[..input_idx]).unwrap_or("");
                    match cmd {
                        "help" => {
                            let _ = uart.write_str("Available commands:\r\n");
                            let _ = uart.write_str("  help    - Show this help\r\n");
                            let _ = uart.write_str("  led on  - Turn LED on\r\n");
                            let _ = uart.write_str("  led off - Turn LED off\r\n");
                            let _ = uart.write_str("  info    - Show system info\r\n");
                        }
                        "led on" => {
                            led_pin.set_high().unwrap();
                            let _ = uart.write_str("LED is ON\r\n");
                        }
                        "led off" => {
                            led_pin.set_low().unwrap();
                            let _ = uart.write_str("LED is OFF\r\n");
                        }
                        "info" => {
                            let _ = uart.write_str("System: Raspberry Pi Pico series\r\n");
                            #[cfg(rp2040)]
                            let _ = uart.write_str("Chip: RP2040\r\n");
                            #[cfg(rp2350)]
                            let _ = uart.write_str("Chip: RP2350\r\n");
                        }
                        _ => {
                            let _ = uart.write_str("Unknown command: ");
                            let _ = uart.write_str(cmd);
                            let _ = uart.write_str("\r\n");
                        }
                    }
                    input_idx = 0;
                }
                let _ = uart.write_str("> ");
            } else if c == 0x08 || c == 0x7F {
                // Backspace
                if input_idx > 0 {
                    input_idx -= 1;
                    let _ = uart.write_str("\x08 \x08");
                }
            } else if input_idx < input_buf.len() {
                input_buf[input_idx] = c;
                input_idx += 1;
            }
        }
    }
}

/// Program metadata for `picotool info`
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [hal::binary_info::EntryAddr; 5] = [
    hal::binary_info::rp_cargo_bin_name!(),
    hal::binary_info::rp_cargo_version!(),
    hal::binary_info::rp_program_description!(c"Blinky Example"),
    hal::binary_info::rp_cargo_homepage_url!(),
    hal::binary_info::rp_program_build_attribute!(),
];

// End of file
