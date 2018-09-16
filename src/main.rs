#![no_main]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt as rt;
extern crate panic_semihosting;
extern crate stm32f103xx_hal as hal;
extern crate embedded_graphics;
extern crate embedded_hal;

use hal::prelude::*;
use hal::spi::{Mode, Spi, Phase::*, Polarity::*};
use rt::{entry, exception, ExceptionFrame};
use embedded_graphics::coord::Coord;
use embedded_graphics::fonts::Font6x8;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line};
use core::marker::PhantomData;

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
    let mut spi = Spi::spi2(
        dp.SPI2,
        (sck, miso, mosi),
        Mode { polarity: IdleLow, phase: CaptureOnFirstTransition },
        4.mhz(),
        clocks,
        &mut rcc.apb1,
    );
    
    let mut il3820 = Il3820::new(
        &mut spi,
        gpiob.pb12.into_push_pull_output(&mut gpiob.crh),
        gpioa.pa8.into_push_pull_output(&mut gpioa.crh),
        gpioa.pa9.into_push_pull_output(&mut gpioa.crh),
        gpioa.pa10.into_floating_input(&mut gpioa.crh),
        &mut delay,
    ).unwrap();

    loop {
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
        il3820.set_display(&mut spi, &display).unwrap();
        il3820.update(&mut spi).unwrap();

        delay.delay_ms(1_000u16);
    }
}

pub struct Display([u8; 296 * 128 / 8]);
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


pub struct Il3820<S, N, D, R, B> {
    _spi: PhantomData<S>,
    nss: N,
    dc: D,
    rst: R,
    busy: B,
}
impl<S, N, D, R, B> Il3820<S, N, D, R, B>
    where
    S: embedded_hal::blocking::spi::Write<u8>,
    N: embedded_hal::digital::OutputPin,
    D: embedded_hal::digital::OutputPin,
    R: embedded_hal::digital::OutputPin,
    B: embedded_hal::digital::InputPin,
{
    pub fn new<DELAY: embedded_hal::blocking::delay::DelayMs<u8>>(
        spi: &mut S,
        nss: N,
        dc: D,
        rst: R,
        busy: B,
        delay: &mut DELAY
    ) -> Result<Self, S::Error> {
        let mut res = Self {
            _spi: PhantomData::default(),
            nss,
            dc,
            rst,
            busy,
        };

        // reset
        res.rst.set_low();
        delay.delay_ms(200u8);
        res.rst.set_high();
        delay.delay_ms(200u8);

        res.cmd_with_data(spi, 0x01, &[0x27, 0x01, 0x00])?;//GDOControl (screen width)
        res.cmd_with_data(spi, 0x0c, &[0xd7, 0xd6, 0x9d])?;// softstart
        res.cmd_with_data(spi, 0x2C, &[0xA8])?;//VCOMVol
        res.cmd_with_data(spi, 0x3a, &[0x1a])?;//dummy line
        res.cmd_with_data(spi, 0x3b, &[0x08])?;//Gate Time
        res.cmd_with_data(spi, 0x32, &LUT_FULL)?;// set LUT

        // power on
        res.cmd_with_data(spi, 0x22, &[0xc3])?;
        res.cmd(spi, 0x20)?;

        res.cmd_with_data(spi, 0x11, &[3])?;//ram data entry mode

        // set ram area
        res.cmd_with_data(spi, 0x44, &[0, 127 / 8])?;// x
        res.cmd_with_data(spi, 0x45, &[0, 0, 39, 1])?;// y

        // set ram ptr
        res.cmd_with_data(spi, 0x4e, &[0])?;// x
        res.cmd_with_data(spi, 0x4f, &[0, 0])?;// y

        Ok(res)
    }
    pub fn set_display(&mut self, spi: &mut S, display: &Display) -> Result<(), S::Error> {
        // set ram
        self.cmd_with_data(spi, 0x24, &display.0)
    }
    pub fn update(&mut self, spi: &mut S) -> Result<(), S::Error> {
        self.cmd_with_data(spi, 0x22, &[0xc4])?;
        self.cmd(spi, 0x20)?;
        self.cmd(spi, 0xff)
    }
    pub fn power_off(&mut self, spi: &mut S) -> Result<(), S::Error> {
        self.cmd_with_data(spi, 0x22, &[0xc3])?;
        self.cmd(spi, 0x20)
    }
    fn cmd(&mut self, spi: &mut S, c: u8) -> Result<(), S::Error> {
        self.nss.set_low();
        while self.busy.is_high() {}
        self.dc.set_low();
        spi.write(&[c])?;
        self.nss.set_high();
        Ok(())
    }
    fn write_data(&mut self, spi: &mut S, data: &[u8]) -> Result<(), S::Error> {
        self.nss.set_low();
        self.dc.set_high();
        spi.write(data)?;
        self.nss.set_high();
        Ok(())
    }
    fn cmd_with_data(&mut self, spi: &mut S, c: u8, data: &[u8]) -> Result<(), S::Error> {
        self.cmd(spi, c)?;
        self.write_data(spi, data)
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
