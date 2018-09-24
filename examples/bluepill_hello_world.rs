#![no_main]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt as rt;
extern crate embedded_graphics;
extern crate embedded_hal;
extern crate il3820;
extern crate panic_semihosting;
extern crate stm32f103xx_hal as hal;

use embedded_graphics::coord::Coord;
use embedded_graphics::fonts::Font6x8;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line};
use hal::prelude::*;
use hal::spi::Spi;
use rt::{entry, exception, ExceptionFrame};

#[entry]
fn entry() -> ! {
    main()
}
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
        il3820::MODE,
        4.mhz(),
        clocks,
        &mut rcc.apb1,
    );

    let mut il3820 = il3820::Il3820::new(
        &mut spi,
        gpiob.pb12.into_push_pull_output(&mut gpiob.crh),
        gpioa.pa8.into_push_pull_output(&mut gpioa.crh),
        gpioa.pa9.into_push_pull_output(&mut gpioa.crh),
        gpioa.pa10.into_floating_input(&mut gpioa.crh),
        &mut delay,
    );
    il3820.clear(&mut spi).unwrap();

    let mut i = 0;
    loop {
        i += 1;
        let mut display = il3820::DisplayRibbonLeft::default();
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
                .translate(Coord::new(5 + i, 50))
                .into_iter(),
        );
        if i % 20 == 9 {
            il3820.set_full();
        }
        il3820.set_display(&mut spi, &display).unwrap();
        il3820.update(&mut spi).unwrap();
        il3820.set_partial();
        if i > 296 {
            i = 0;
        }
        delay.delay_ms(1_000u16);
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
