 #![no_std]
 #![no_main]
 #![feature(type_alias_impl_trait)]

extern crate alloc;

use display_interface::WriteOnlyDataCommand;
use embedded_graphics::{
    mono_font::{ascii::FONT_8X13, MonoTextStyle}, pixelcolor::{Rgb565, RgbColor}, prelude::*, text::Text };
use emulator::io::FileAsset;
use esp_backtrace as _;
use esp_println as _;
use rustzx_core::{ 
    host::Host, zx::{machine::ZXMachine, video::colors::{ZXBrightness, ZXColor}}, EmulationMode, Emulator, RustzxSettings
};
use pc_keyboard::{ layouts, HandleControl, ScancodeSet2 };

mod host;
mod stopwatch;
mod zx_event;
use zx_event::Event;
mod pc_zxkey;
use pc_zxkey::{ pc_code_to_zxkey, pc_code_to_modifier };

use core::{cell::RefCell, mem::MaybeUninit, ptr::read, time::Duration};
use embedded_hal_bus::spi::RefCellDevice;
use display_interface_spi::SPIInterface;
use embassy_executor::Spawner;
use esp_hal::{
    prelude::*,
    Delay,
    embassy,
    clock::ClockControl,
    gpio::{ self, IO },
    psram,
    spi::{ master::Spi, SpiMode },
    timer::TimerGroup,
    peripherals::{ Peripherals, UART1 },
    uart::{ Uart, config::{ Config, DataBits, Parity, StopBits }, TxRxPins },
};
use esp_bsp::{
    BoardType,
    lcd_gpios
};
use mipidsi::{
    Builder,
    models::Model,
    options::{ColorOrder, Orientation, Rotation}
};
use log::{info, error, debug};

#[global_allocator]
static ALLOCATOR : esp_alloc::EspHeap = esp_alloc::EspHeap::empty();

fn init_heap(){
    unsafe {
        ALLOCATOR.init(psram::psram_vaddr_start() as *mut u8, psram::PSRAM_BYTES);
    }
}

const SCREEN_OFFSET_X : u16 = (320 - 256) / 2;
const SCREEN_OFFSET_Y : u16 = (320 - 256) / 2;

const ZX_BLACK          : Rgb565 = Rgb565::BLACK;
const ZX_BRIGHT_BLUE    : Rgb565 = Rgb565::new(0, 0, Rgb565::MAX_B);
const ZX_BRIGHT_RED     : Rgb565 = Rgb565::new(Rgb565::MAX_R, 0, 0);
const ZX_BRIGHT_PURPLE  : Rgb565 = Rgb565::new(Rgb565::MAX_R, 0, Rgb565::MAX_B);
const ZX_BRIGHT_GREEN   : Rgb565 = Rgb565::new(0, Rgb565::MAX_B, 0);
const ZX_BRIGHT_CYAN    : Rgb565 = Rgb565::new(0, Rgb565::MAX_G, Rgb565::MAX_B);
const ZX_BRIGHT_YELLOW  : Rgb565 = Rgb565::new(Rgb565::MAX_R, Rgb565::MAX_G, 0);
const ZX_BRIGHT_WHITE   : Rgb565 = Rgb565::WHITE;
const ZX_NORMAL_BLUE    : Rgb565 = Rgb565::new(0, 0, Rgb565::MAX_B/2);
const ZX_NORMAL_RED     : Rgb565 = Rgb565::new(Rgb565::MAX_R/2, 0, 0);
const ZX_NORMAL_PURPLE  : Rgb565 = Rgb565::new(Rgb565::MAX_R/2, 0, Rgb565::MAX_B/2);
const ZX_NORMAL_GREEN   : Rgb565 = Rgb565::new(0, Rgb565::MAX_B/2, 0);
const ZX_NORMAL_CYAN    : Rgb565 = Rgb565::new(0, Rgb565::MAX_G/2, Rgb565::MAX_B/2);
const ZX_NORMAL_YELLOW  : Rgb565 = Rgb565::new(Rgb565::MAX_R/2, Rgb565::MAX_G/2, 0);
const ZX_NORMAL_WHITE   : Rgb565 = Rgb565::new(Rgb565::MAX_R/2, Rgb565::MAX_G/2, Rgb565::MAX_B/2);

fn colour_conv(colour : &ZXColor, brightness: ZXBrightness) -> Rgb565 {
    match(colour, brightness) {
        (ZXColor::Black, _) => ZX_BLACK,
        (ZXColor::Blue, ZXBrightness::Bright) => ZX_BRIGHT_BLUE,
        (ZXColor::Red, ZXBrightness::Bright) => ZX_BRIGHT_RED,
        (ZXColor::Purple, ZXBrightness::Bright) => ZX_BRIGHT_PURPLE,
        (ZXColor::Green, ZXBrightness::Bright) => ZX_BRIGHT_GREEN,
        (ZXColor::Cyan, ZXBrightness::Bright) => ZX_BRIGHT_CYAN,
        (ZXColor::Yellow, ZXBrightness::Bright) => ZX_BRIGHT_YELLOW,
        (ZXColor::White, ZXBrightness::Bright) => ZX_BRIGHT_WHITE,
        (ZXColor::Blue, ZXBrightness::Normal) => ZX_NORMAL_BLUE,
        (ZXColor::Red, ZXBrightness::Normal) => ZX_NORMAL_RED,
        (ZXColor::Purple, ZXBrightness::Normal) => ZX_NORMAL_PURPLE,
        (ZXColor::Green, ZXBrightness::Normal) => ZX_NORMAL_GREEN,
        (ZXColor::Cyan, ZXBrightness::Normal) => ZX_NORMAL_CYAN,
        (ZXColor::Yellow, ZXBrightness::Normal) => ZX_NORMAL_YELLOW,
        (ZXColor::White, ZXBrightness::Normal) => ZX_NORMAL_WHITE,
    }
}
fn handle_key_event<H: Host>(key : pc_keyboard::KeyCode, state : pc_keyboard::KeyState, emulator: &mut Emulator<H>){
    let is_pressed = matches!(state, pc_keyboard::KeyState::Down);
    if let Some(mapped_key) = pc_code_to_zxkey(key, is_pressed).or_else(|| pc_code_to_modifier(key, is_pressed)){
        match mapped_key {
            Event::ZXKey(k, p) => {
                debug!("-> ZXKey");
                emulator.send_key(k, p);
            },
            Event::NoEvent => {
                error!("Key not implemented");
            },
            Event::ZXKeyWithModifier(k, k2, p) => {
                debug!("-> ZXKeyWithModifier");
                emulator.send_key(k, p);
                emulator.send_key(k2, p);
            }
        }
    } else {
        info!("Mapped key: No event");
    }
}

 #[main]
 async fn main(spawner : Spawner) -> ! {
    let peripherals = Peripherals::take();
    psram::init_psram(peripherals.PSRAM);
    init_heap();

    let system = peripherals.SYSTEM.split();
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();
    let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);

    let timg0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    embassy::init(&clocks, timg0);

    let mut delay = Delay::new(&clocks);

    let (lcd_sclk, lcd_mosi, lcd_cs, lcd_miso, lcd_dc, mut lcd_backlight, lcd_reset) = lcd_gpios!(BoardType::ESP32CheapYellowDisplay, io);
    let serial_tx = io.pins.gpio25;
    let serial_rx = io.pins.gpio26;
    //let mut led = io.pins.gpio16.into_push_pull_output();

    esp_println::logger::init_logger_from_env();

    // Display
    info!("Starting display");
    let spi = Spi::new(
        peripherals.SPI2,
        1000_u32.kHz(),
        SpiMode::Mode0,
        &clocks)
        .with_pins(
            Some(lcd_sclk),
            Some(lcd_mosi),
            Some(lcd_miso),
            gpio::NO_PIN
        );
    let spi_bus = RefCell::new(spi);
    let spi_dev = RefCellDevice::new_no_delay(&spi_bus, lcd_cs.into_push_pull_output());
    let di = SPIInterface::new(spi_dev, lcd_dc);
    let mut display = Builder::new(mipidsi::models::ILI9341Rgb565, di)
        .color_order(ColorOrder::Rgb)
        .orientation(Orientation::new().rotate(Rotation::Deg90).flip_horizontal())
        .reset_pin(lcd_reset)
        .init(&mut delay)
        .expect("Display init");
    let _ = lcd_backlight.set_high();
    info!("Initialising...");
    Text::new(
        "Initialising...",
        Point::new(80, 110),
        MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE))
        .draw(&mut display)
        .unwrap();
    info!("Initialised");

    info!("Starting uart");
    let config = Config {
        baudrate: 115200,
        data_bits: DataBits::DataBits8,
        parity: Parity::ParityNone,
        stop_bits: StopBits::STOP1,
    };
    let pins = TxRxPins::new_tx_rx(serial_tx, serial_rx);
    let mut serial = Uart::new_with_config(peripherals.UART1, config, Some(pins), &clocks);

    info!("Creating emulator");
    let settings = RustzxSettings {
        machine : ZXMachine::Sinclair48K,
        emulation_mode : EmulationMode::FrameCount(1),
        tape_fastload_enabled : true,
        kempston_enabled : false,
        mouse_enabled : false,
        load_default_rom : true };
    
    info!("Initialising emulator");
    const MAX_FRAME_DURATION : Duration = Duration::from_millis(0);

    let mut emulator : Emulator<host::Esp32Host> =
        match Emulator::new(settings, host::Esp32HostContext {}){
            Ok(emulator) => emulator,
            Err(err) => {
                error!("Error creating emulator = {}", err);
                panic!();
            }
        };

    info!("Loading tape");
    let tape_bytes = include_bytes!("../../data/hello.tap");
    let tape_asset = FileAsset::new(tape_bytes);
    let _ = emulator.load_tape(rustzx_core::host::Tape::Tap(tape_asset));

    info!("Setting up keyboard");
    let mut kb = pc_keyboard::Keyboard::new(
        ScancodeSet2::new(),
        layouts::Us104Key,
        HandleControl::MapLettersToUnicode
    );

    loop {
        info!("Handling serial");
        let read_result = serial.read();
        match read_result {
            Ok(byte) => {
                match kb.add_byte(byte) {
                    Ok(Some(event)) => {
                        info!("Event {:?}", event);
                        handle_key_event(event.code, event.state, &mut emulator);
                    },
                    Ok(None) => {},
                    Err(_) => {},
                }
            },
            Err(_) => {},
        }


        info!("Emulating frame");
        match emulator.emulate_frames(MAX_FRAME_DURATION){
            Ok(_) => {
                let framebuffer = emulator.screen_buffer();
                if let (Some(top_left), Some(bottom_right)) = (framebuffer.bounding_box_top_left, framebuffer.bounding_box_bottom_right) {
                    let pixel_iterator = framebuffer.get_region_pixel_iter(top_left, bottom_right);
                    info!("Updating display {}x{} - {}x{}", top_left.0, top_left.1, bottom_right.0, bottom_right.1);
                    let _ = display.set_pixels(
                        top_left.0 as u16  + SCREEN_OFFSET_X,
                        top_left.1 as u16 + SCREEN_OFFSET_Y,
                        bottom_right.0 as u16 + SCREEN_OFFSET_X,
                        bottom_right.1 as u16+ SCREEN_OFFSET_Y,
                        pixel_iterator);
                }
                emulator.reset_bounding_box();
            },
            _ => {
                error!("Emulation of frame failed!");
            }
        }

        //led.toggle().expect("Failed to toggle led");
    }
 }
