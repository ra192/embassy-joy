#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{
    ADCPrescaler, APBPrescaler, AHBPrescaler, Hse, HseMode, Pll, PllMul, PllPreDiv, PllSource,
    Sysclk,
};
use embassy_stm32::time::Hertz;
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();

    // HSE: external 8 MHz crystal
    config.rcc.hse = Some(Hse {
        freq: Hertz(8_000_000),
        mode: HseMode::Oscillator,
    });

    // PLL: 8 MHz / 1 * 9 = 72 MHz
    config.rcc.pll = Some(Pll {
        src: PllSource::HSE,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL9,
    });
    config.rcc.sys = Sysclk::PLL1_P;

    // HCLK = 72 MHz
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    // APB1 = 36 MHz (max for F103)
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    // APB2 = 72 MHz
    config.rcc.apb2_pre = APBPrescaler::DIV1;
    // ADC = 12 MHz (< 14 MHz max)
    config.rcc.adc_pre = ADCPrescaler::DIV6;

    // USB: with PLL @ 72 MHz embassy sets USBPRE=DIV1_5 -> USBCLK = 48 MHz automatically.

    let p = embassy_stm32::init(config);
    info!("Hello World!");

    let mut led = Output::new(p.PC13, Level::High, Speed::Low);

    loop {
        info!("led on!");
        led.set_high();
        Timer::after_millis(500).await;

        info!("led off!");
        led.set_low();
        Timer::after_millis(500).await;
    }
}
