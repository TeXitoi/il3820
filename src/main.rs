#![no_main]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt as rt;
extern crate panic_semihosting;
extern crate stm32f103xx_hal as hal;
extern crate embedded_graphics;

use hal::prelude::*;
use hal::spi::{Mode, Spi, Phase::*, Polarity::*};
use rt::{entry, exception, ExceptionFrame};
use embedded_graphics::coord::Coord;
use embedded_graphics::fonts::Font6x8;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line};

#[entry]
fn main() -> ! {
    let dp = hal::stm32f103xx::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();
    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();
    let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
    let mut gpiob = dp.GPIOB.split(&mut rcc.apb2);
    let clocks = rcc.cfgr.freeze(&mut flash.acr);
    let mut delay = hal::delay::Delay::new(cp.SYST, clocks);

    let sck = gpiob.pb13.into_alternate_push_pull(&mut gpiob.crh);
    let miso = gpiob.pb14;
    let mosi = gpiob.pb15.into_alternate_push_pull(&mut gpiob.crh);
    let spi = Spi::spi2(
        dp.SPI2,
        (sck, miso, mosi),
        Mode { polarity: IdleLow, phase: CaptureOnFirstTransition },
        4.mhz(),
        clocks,
        &mut rcc.apb1,
    );
    
    let mut il3820 = Il3820 {
        spi,
        nss: gpiob.pb12.into_push_pull_output(&mut gpiob.crh),
        dc: gpioa.pa8.into_push_pull_output(&mut gpioa.crh),
        rst: gpioa.pa9.into_push_pull_output(&mut gpioa.crh),
        busy: gpioa.pa10.into_floating_input(&mut gpioa.crh),
    };

    loop {
        il3820.rst.set_low();
        delay.delay_ms(200u16);
        il3820.rst.set_high();
        delay.delay_ms(200u16);
        il3820.nss.set_low();

        il3820.cmd_with_data(0x01, &[0x27, 0x01, 0x00]).unwrap();//GDOControl (screen width)
        il3820.cmd_with_data(0x0c, &[0xd7, 0xd6, 0x9d]).unwrap();// softstart
        il3820.cmd_with_data(0x2C, &[0xA8]).unwrap();//VCOMVol
        il3820.cmd_with_data(0x3a, &[0x1a]).unwrap();//dummy line
        il3820.cmd_with_data(0x3b, &[0x08]).unwrap();//Gate Time
        il3820.cmd_with_data(0x11, &[3]).unwrap();//ram data entry mode

        // set ram area
        il3820.cmd_with_data(0x44, &[0, 127 / 8]).unwrap();// x
        il3820.cmd_with_data(0x45, &[0, 0, 39, 1]).unwrap();// y

        // set ram ptr
        il3820.cmd_with_data(0x4e, &[0]).unwrap();// x
        il3820.cmd_with_data(0x4f, &[0, 0]).unwrap();// y

        // set LUT
        il3820.cmd_with_data(0x32, &LUT_FULL).unwrap();

        // power on
        il3820.cmd_with_data(0x22, &[0xc3]).unwrap();
        il3820.cmd(0x20).unwrap();

        // set ram with funky data
        il3820.cmd(0x24).unwrap();
        let mut display = Display::default();
        display.draw(
            Circle::new(Coord::new(64, 64), 64)
                .with_stroke(Some(1u8.into()))
                .into_iter(),
        );
        display.draw(
            Line::new(Coord::new(64, 64), Coord::new(0, 64))
                .with_stroke(Some(1u8.into()))
                .into_iter(),
        );
        display.draw(
            Line::new(Coord::new(64, 64), Coord::new(80, 80))
                .with_stroke(Some(1u8.into()))
                .into_iter(),
        );
        display.draw(
            Font6x8::render_str("Hello World!")
                .with_stroke(Some(1u8.into()))
                .translate(Coord::new(5, 50))
                .into_iter(),
        );
        il3820.write_data(&display.0).unwrap();

        // update full
        il3820.cmd_with_data(0x22, &[0xc4]).unwrap();
        il3820.cmd(0x20).unwrap();
        il3820.cmd(0xff).unwrap();

        // power off
        il3820.cmd_with_data(0x22, &[0xc3]).unwrap();
        il3820.cmd(0x20).unwrap();

        il3820.nss.set_high();
        delay.delay_ms(1_000u16);
    }
}

struct Display([u8; 296 * 128 / 8]);
impl Default for Display {
    fn default() -> Self {
        Display([0xff; 296 * 128 / 8])
    }
}
impl Drawing<u8> for Display {
    fn draw<T>(&mut self, item_pixels: T)
    where
        T: Iterator<Item = Pixel<u8>>
    {
        for Pixel(UnsignedCoord(x, y), color) in item_pixels {
            if x > 127 || y > 295 { continue; }
            let cell = &mut self.0[x as usize / 8 + (y as usize) * 128 / 8];
            let bit = 7 - x % 8;
            if color != 0 {
                *cell &= !(1 << bit);
            } else {
                *cell |= 1 << bit;
            }
        }
    }
}

static LUT_FULL: [u8; 30] = [
    0x02, 0x02, 0x01, 0x11, 0x12, 0x12, 0x22, 0x22, 
    0x66, 0x69, 0x69, 0x59, 0x58, 0x99, 0x99, 0x88, 
    0x00, 0x00, 0x00, 0x00, 0xF8, 0xB4, 0x13, 0x51, 
    0x35, 0x51, 0x51, 0x19, 0x01, 0x00
];
struct Il3820 {
    spi: hal::spi::Spi<hal::stm32f103xx::SPI2, (hal::gpio::gpiob::PB13<hal::gpio::Alternate<hal::gpio::PushPull>>, hal::gpio::gpiob::PB14<hal::gpio::Input<hal::gpio::Floating>>, hal::gpio::gpiob::PB15<hal::gpio::Alternate<hal::gpio::PushPull>>)>,
    nss: hal::gpio::gpiob::PB12<hal::gpio::Output<hal::gpio::PushPull>>,
    dc: hal::gpio::gpioa::PA8<hal::gpio::Output<hal::gpio::PushPull>>,
    rst: hal::gpio::gpioa::PA9<hal::gpio::Output<hal::gpio::PushPull>>,
    busy: hal::gpio::gpioa::PA10<hal::gpio::Input<hal::gpio::Floating>>,
}
impl Il3820 {
    fn cmd(&mut self, c: u8) -> Result<(), hal::spi::Error> {
        while self.busy.is_high() {}
        self.dc.set_low();
        self.spi.write(&[c])?;
        Ok(())
    }
    fn write_data(&mut self, data: &[u8]) -> Result<(), hal::spi::Error> {
        self.dc.set_high();
        self.spi.write(data)
    }
    fn cmd_with_data(&mut self, c: u8, data: &[u8]) -> Result<(), hal::spi::Error> {
        self.cmd(c)?;
        self.write_data(data)
    }
}

#[exception]
fn HardFault(ef: &ExceptionFrame) -> ! {
    panic!("{:#?}", ef);
}

#[exception]
fn DefaultHandler(irqn: i16) {
    panic!("Unhandled exception (IRQn = {})", irqn);
}
