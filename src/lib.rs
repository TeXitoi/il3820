#![no_std]

extern crate embedded_graphics;
extern crate embedded_hal;

use core::marker::PhantomData;
use embedded_graphics::prelude::*;
use embedded_hal::spi;

pub const MODE: spi::Mode = spi::Mode {
    polarity: spi::Polarity::IdleLow,
    phase: spi::Phase::CaptureOnFirstTransition,
};

pub trait GetBuf {
    fn get_buf(&self) -> &[u8];
}

pub struct DisplayRibbonButton([u8; 296 * 128 / 8]);
impl Default for DisplayRibbonButton {
    fn default() -> Self {
        DisplayRibbonButton([0xff; 296 * 128 / 8])
    }
}
impl GetBuf for DisplayRibbonButton {
    fn get_buf(&self) -> &[u8] {
        &self.0
    }
}
impl Drawing<u8> for DisplayRibbonButton {
    fn draw<T>(&mut self, item_pixels: T)
    where
        T: Iterator<Item = Pixel<u8>>,
    {
        for Pixel(UnsignedCoord(x, y), color) in item_pixels {
            if x > 127 || y > 295 {
                continue;
            }
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

pub struct DisplayRibbonLeft([u8; 296 * 128 / 8]);
impl Default for DisplayRibbonLeft {
    fn default() -> Self {
        DisplayRibbonLeft([0xff; 296 * 128 / 8])
    }
}
impl GetBuf for DisplayRibbonLeft {
    fn get_buf(&self) -> &[u8] {
        &self.0
    }
}
impl Drawing<u8> for DisplayRibbonLeft {
    fn draw<T>(&mut self, item_pixels: T)
    where
        T: Iterator<Item = Pixel<u8>>,
    {
        for Pixel(UnsignedCoord(x, y), color) in item_pixels {
            if y > 127 || x > 295 {
                continue;
            }
            let cell = &mut self.0[y as usize / 8 + (295 - x as usize) * 128 / 8];
            let bit = 7 - y % 8;
            if color != 0 {
                *cell &= !(1 << bit);
            } else {
                *cell |= 1 << bit;
            }
        }
    }
}

static LUT_FULL: [u8; 30] = [
    0x50, 0xAA, 0x55, 0xAA, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
static LUT_PART: [u8; 30] = [
    0x10, 0x18, 0x18, 0x08, 0x18, 0x18, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x13, 0x14, 0x44, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

pub struct Il3820<S, N, D, R, B> {
    _spi: PhantomData<S>,
    nss: N,
    dc: D,
    rst: R,
    busy: B,
    partial: bool,
    is_inited: bool,
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
        _spi: &mut S,
        nss: N,
        dc: D,
        rst: R,
        busy: B,
        delay: &mut DELAY,
    ) -> Self {
        let mut res = Self {
            _spi: PhantomData::default(),
            nss,
            dc,
            rst,
            busy,
            partial: true,
            is_inited: false,
        };
        res.reset(delay);
        res
    }
    pub fn reset<DELAY: embedded_hal::blocking::delay::DelayMs<u8>>(&mut self, delay: &mut DELAY) {
        self.rst.set_low();
        delay.delay_ms(1u8);
        self.rst.set_high();
        delay.delay_ms(1u8);
    }
    pub fn set_partial(&mut self) {
        if !self.partial {
            self.partial = true;
            self.is_inited = false;
        }
    }
    pub fn set_full(&mut self) {
        if self.partial {
            self.partial = false;
            self.is_inited = false;
        }
    }
    pub fn clear(&mut self, spi: &mut S) -> Result<(), S::Error> {
        let partial = self.partial;
        self.partial = false;
        self.init(spi)?;
        if !self.is_inited {
            self.init(spi)?;
        }
        self.cmd(spi, 0x24)?;
        for _ in 0..128 / 8 * 296 {
            self.write_data(spi, &[0xFF])?;
        }
        self.update(spi)?;
        if partial != self.partial {
            self.partial = partial;
            self.is_inited = false;
        }
        Ok(())
    }
    pub fn set_display<DISPLAY: GetBuf>(
        &mut self,
        spi: &mut S,
        display: &DISPLAY,
    ) -> Result<(), S::Error> {
        if !self.is_inited {
            self.init(spi)?;
        }
        self.cmd_with_data(spi, 0x24, display.get_buf())
    }
    pub fn update(&mut self, spi: &mut S) -> Result<(), S::Error> {
        if !self.is_inited {
            self.init(spi)?;
        }
        self.cmd_with_data(spi, 0x22, &[0xc4])?;
        self.cmd(spi, 0x20)?;
        self.cmd(spi, 0xff)
    }
    pub fn power_off(&mut self, spi: &mut S) -> Result<(), S::Error> {
        self.cmd_with_data(spi, 0x22, &[0xc3])?;
        self.cmd(spi, 0x20)?;
        self.is_inited = false;
        Ok(())
    }
    fn init(&mut self, spi: &mut S) -> Result<(), S::Error> {
        self.cmd_with_data(spi, 0x01, &[0x27, 0x01, 0x00])?; //GDOControl (screen width)
        self.cmd_with_data(spi, 0x0c, &[0xd7, 0xd6, 0x9d])?; // softstart
        self.cmd_with_data(spi, 0x2c, &[0xa8])?; //VCOMVol
        self.cmd_with_data(spi, 0x3a, &[0x1a])?; //dummy line
        self.cmd_with_data(spi, 0x3b, &[0x08])?; //Gate Time
        self.cmd_with_data(spi, 0x11, &[3])?; //ram data entry mode

        // set ram area
        self.cmd_with_data(spi, 0x44, &[0, 127 / 8])?; // x
        self.cmd_with_data(spi, 0x45, &[0, 0, 39, 1])?; // y

        // set ram ptr
        self.cmd_with_data(spi, 0x4e, &[0])?; // x
        self.cmd_with_data(spi, 0x4f, &[0, 0])?; // y

        if self.partial {
            self.cmd_with_data(spi, 0x32, &LUT_PART)?;
        } else {
            self.cmd_with_data(spi, 0x32, &LUT_FULL)?;
        }

        // power on
        self.cmd_with_data(spi, 0x22, &[0xc3])?;
        self.cmd(spi, 0x20)?;

        self.is_inited = true;

        Ok(())
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
