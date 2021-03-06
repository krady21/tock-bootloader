//! Board file for Hail bootloader.

#![no_std]
#![no_main]
#![feature(asm, const_fn, lang_items, panic_implementation)]

extern crate bootloader;
extern crate cortexm4;
#[macro_use(create_capability, static_init)]
extern crate kernel;
extern crate capsules;
extern crate sam4l;

use core::panic::PanicInfo;

use kernel::capabilities;
use kernel::hil;
use kernel::hil::Controller;
use kernel::Platform;

include!(concat!(env!("OUT_DIR"), "/attributes.rs"));

// No processes are supported.
static mut PROCESSES: [Option<&'static kernel::procs::ProcessType>; 0] = [];

/// Dummy buffer that causes the linker to reserve enough space for the stack.
#[no_mangle]
#[link_section = ".stack_buffer"]
pub static mut STACK_MEMORY: [u8; 0x2000] = [0; 0x2000];

struct HailBootloader {
    bootloader: &'static bootloader::bootloader::Bootloader<
        'static,
        sam4l::usart::USART,
        sam4l::flashcalw::FLASHCALW,
        sam4l::gpio::GPIOPin,
    >,
}

impl Platform for HailBootloader {
    fn with_driver<F, R>(&self, _driver_num: usize, f: F) -> R
    where
        F: FnOnce(Option<&kernel::Driver>) -> R,
    {
        // Bootloader does not support apps.
        f(None)
    }
}

unsafe fn set_pin_primary_functions() {
    use sam4l::gpio::PeripheralFunction::{A, B};
    use sam4l::gpio::{PA, PB};

    PA[04].configure(Some(A)); // A0 - ADC0
    PA[05].configure(Some(A)); // A1 - ADC1
    PA[06].configure(Some(A)); // DAC
    PA[07].configure(None); //... WKP - Wakeup
    PA[08].configure(None); //... Bootloader select pin.
    PA[09].configure(None); //... ACC_INT1 - FXOS8700CQ Interrupt 1
    PA[10].configure(None); //... unused
    PA[11].configure(Some(A)); // FTDI_OUT - USART0 RX FTDI->SAM4L
    PA[12].configure(Some(A)); // FTDI_IN - USART0 TX SAM4L->FTDI
    PA[13].configure(None); //... RED_LED
    PA[14].configure(None); //... BLUE_LED
    PA[15].configure(None); //... GREEN_LED
    PA[16].configure(None); //... BUTTON - User Button
    PA[17].configure(None); //... !NRF_RESET - Reset line for nRF51822
    PA[18].configure(None); //... ACC_INT2 - FXOS8700CQ Interrupt 2
    PA[19].configure(None); //... unused
    PA[20].configure(None); //... !LIGHT_INT - ISL29035 Light Sensor Interrupt
                            // SPI Mode
    PA[21].configure(Some(A)); // D3 - SPI MISO
    PA[22].configure(Some(A)); // D2 - SPI MOSI
    PA[23].configure(Some(A)); // D4 - SPI SCK
    PA[24].configure(Some(A)); // D5 - SPI CS0
                               // // I2C MODE
                               // PA[21].configure(None); // D3
                               // PA[22].configure(None); // D2
                               // PA[23].configure(Some(B)); // D4 - TWIMS0 SDA
                               // PA[24].configure(Some(B)); // D5 - TWIMS0 SCL
                               // UART Mode
    PA[25].configure(Some(B)); // RX - USART2 RXD
    PA[26].configure(Some(B)); // TX - USART2 TXD

    PB[00].configure(Some(A)); // SENSORS_SDA - TWIMS1 SDA
    PB[01].configure(Some(A)); // SENSORS_SCL - TWIMS1 SCL
    PB[02].configure(Some(A)); // A2 - ADC3
    PB[03].configure(Some(A)); // A3 - ADC4
    PB[04].configure(Some(A)); // A4 - ADC5
    PB[05].configure(Some(A)); // A5 - ADC6
    PB[06].configure(Some(A)); // NRF_CTS - USART3 RTS
    PB[07].configure(Some(A)); // NRF_RTS - USART3 CTS
    PB[08].configure(None); //... NRF_INT - Interrupt line nRF->SAM4L
    PB[09].configure(Some(A)); // NRF_OUT - USART3 RXD
    PB[10].configure(Some(A)); // NRF_IN - USART3 TXD
    PB[11].configure(None); //... D6
    PB[12].configure(None); //... D7
    PB[13].configure(None); //... unused
    PB[14].configure(None); //... D0
    PB[15].configure(None); //... D1
}

#[no_mangle]
pub unsafe fn reset_handler() {
    sam4l::init();

    sam4l::pm::PM.setup_system_clock(sam4l::pm::SystemClockSource::PllExternalOscillatorAt48MHz {
        frequency: sam4l::pm::OscillatorFrequency::Frequency16MHz,
        startup_mode: sam4l::pm::OscillatorStartup::SlowStart,
    });

    // Source 32Khz and 1Khz clocks from RC23K (SAM4L Datasheet 11.6.8)
    sam4l::bpm::set_ck32source(sam4l::bpm::CK32Source::RC32K);

    set_pin_primary_functions();

    // Create main kernel object. This contains the main loop function.
    let board_kernel = static_init!(kernel::Kernel, kernel::Kernel::new(&PROCESSES));

    // Initialize USART0 for Uart
    sam4l::usart::USART0.set_mode(sam4l::usart::UsartMode::Uart);

    pub static mut PAGEBUFFER: sam4l::flashcalw::Sam4lPage = sam4l::flashcalw::Sam4lPage::new();

    sam4l::flashcalw::FLASH_CONTROLLER.configure();
    let bootloader = static_init!(
        bootloader::bootloader::Bootloader<
            'static,
            sam4l::usart::USART,
            sam4l::flashcalw::FLASHCALW,
            sam4l::gpio::GPIOPin,
        >,
        bootloader::bootloader::Bootloader::new(
            &sam4l::usart::USART0,
            &mut sam4l::flashcalw::FLASH_CONTROLLER,
            &sam4l::gpio::PA[08],
            &mut PAGEBUFFER,
            &mut bootloader::bootloader::BUF
        )
    );
    hil::uart::UART::set_client(&sam4l::usart::USART0, bootloader);
    hil::flash::HasClient::set_client(&sam4l::flashcalw::FLASH_CONTROLLER, bootloader);

    let hail = HailBootloader {
        bootloader: bootloader,
    };

    let chip = static_init!(sam4l::chip::Sam4l, sam4l::chip::Sam4l::new());

    hail.bootloader.initialize();

    let main_loop_capability = create_capability!(capabilities::MainLoopCapability);
    board_kernel.kernel_loop(&hail, chip, None, &main_loop_capability);
}

/// Panic handler.
#[cfg(not(test))]
#[no_mangle]
#[panic_implementation]
pub unsafe extern "C" fn panic_fmt(_pi: &PanicInfo) -> ! {
    loop {}
}
