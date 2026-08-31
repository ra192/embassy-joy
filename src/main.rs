#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{
    ADCPrescaler, AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPreDiv, PllSource,
    Sysclk,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::usb::{Driver, InterruptHandler};
use embassy_stm32::{Config, bind_interrupts, peripherals};
use embassy_time::Timer;
use embassy_usb::Builder;
use embassy_usb::class::hid::{
    Config as HidConfig, HidBootProtocol, HidSubclass, HidWriter, State,
};
use panic_probe as _;
use usbd_hid::descriptor::{AsInputReport, SerializedDescriptor, gen_hid_descriptor};

bind_interrupts!(struct Irqs {
    USB_LP_CAN1_RX0 => InterruptHandler<peripherals::USB>;
});

/// 64 buttons + absolute X/Y axes.
#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = JOYSTICK) = {
        (usage_page = BUTTON, usage_min = BUTTON_1, usage_max = 32) = {
            #[packed_bits = 32] #[item_settings(data,variable,absolute)] buttons_low=input;
        };
        (usage_page = BUTTON, usage_min = 33, usage_max = 64) = {
            #[packed_bits = 32] #[item_settings(data,variable,absolute)] buttons_high=input;
        };
        (collection = PHYSICAL, usage = POINTER) = {
            (usage_page = GENERIC_DESKTOP, usage = X) = {
                #[item_settings(data,variable,absolute)] x=input;
            };
            (usage_page = GENERIC_DESKTOP, usage = Y) = {
                #[item_settings(data,variable,absolute)] y=input;
            };
        };
    }
)]
struct JoystickReport {
    buttons_low: u32,
    buttons_high: u32,
    /// centered at 128
    x: u8,
    /// centered at 128
    y: u8,
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();

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

    let p = embassy_stm32::init(config);

    info!("Joystick initialized");

    let mut led = Output::new(p.PC13, Level::High, Speed::Low);

    // PA12 = D+, PA11 = D-
    let driver = Driver::new(p.USB, Irqs, p.PA12, p.PA11);

    let mut usb_config = embassy_usb::Config::new(0x16c0, 0x27dd);
    usb_config.composite_with_iads = false;
    usb_config.device_class = 0x00;
    usb_config.device_sub_class = 0x00;
    usb_config.device_protocol = 0x00;
    usb_config.manufacturer = Some("STMicroelectronics");
    usb_config.product = Some("STM32 HID Joystick");
    usb_config.serial_number = Some("12345678");

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [],
        &mut control_buf,
    );

    let hid_config = HidConfig {
        report_descriptor: JoystickReport::desc(),
        request_handler: None,
        poll_ms: 10,
        max_packet_size: 16,
        hid_subclass: HidSubclass::No,
        hid_boot_protocol: HidBootProtocol::None,
    };

    let mut writer: HidWriter<'_, _, 16> = HidWriter::new(&mut builder, &mut state, hid_config);

    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    // Do stuff with the class!
    let hid_fut = async {
        let mut btn = 0u8;
        let mut phase = 0i16;
        loop {
            // triangle-wave sweep of X between -100 and 100, Y centered
            let t = phase * 25 - 100;
            let x = (t.clamp(-100, 100) + 128) as u8;
            let y: u8 = 128;
            phase += 1;
            if phase > 8 {
                phase = -8;
                btn = if btn >= 63 { 0 } else { btn + 1 };
            }

            let (buttons_low, buttons_high) = if btn < 32 {
                (1u32 << btn, 0u32)
            } else {
                (0u32, 1u32 << (btn - 32))
            };
            match writer
                .write_serialize(&JoystickReport {
                    buttons_low,
                    buttons_high,
                    x,
                    y,
                })
                .await
            {
                Ok(()) => led.set_low(),
                Err(_) => led.set_high(),
            }

            Timer::after_millis(10).await;
        }
    };

    join(usb_fut, hid_fut).await;
}
