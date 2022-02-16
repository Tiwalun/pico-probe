use dap_rs::{swj::Swj, *};
use embedded_hal::{
    blocking::delay::DelayUs,
    digital::v2::{InputPin, OutputPin, PinState},
};
use rp_pico::hal::gpio::DynPin;

pub struct Context {
    max_frequency: u32,
    cpu_frequency: u32,
    cycles_per_us: u32,
    half_period_ticks: u32,
    swdio: DynPin,
    swclk: DynPin,
    nreset: DynPin,
}

impl dap::DapContext for Context {
    fn high_impedance_mode(&mut self) {
        self.swdio.into_floating_disabled();
        self.swclk.into_floating_disabled();
        self.nreset.into_floating_disabled();
    }
}

impl Context {
    pub fn from_pins(swdio: DynPin, swclk: DynPin, nreset: DynPin, cpu_frequency: u32) -> Self {
        let max_frequency = 100_000;
        let half_period_ticks = cpu_frequency / max_frequency / 2;
        Context {
            max_frequency,
            cpu_frequency,
            cycles_per_us: cpu_frequency / 1_000_000,
            half_period_ticks,
            swdio,
            swclk,
            nreset,
        }
    }
}

impl swj::Swj for Context {
    fn pins(&mut self, output: swj::Pins, mask: swj::Pins, wait_us: u32) -> swj::Pins {
        if mask.contains(swj::Pins::SWCLK) {
            self.swclk.into_push_pull_output();
            self.swclk
                .set_state(if output.contains(swj::Pins::SWCLK) {
                    PinState::High
                } else {
                    PinState::Low
                })
                .ok();
        }

        if mask.contains(swj::Pins::SWDIO) {
            self.swdio.into_push_pull_output();
            self.swdio
                .set_state(if output.contains(swj::Pins::SWDIO) {
                    PinState::High
                } else {
                    PinState::Low
                })
                .ok();
        }

        if mask.contains(swj::Pins::NRESET) {
            if output.contains(swj::Pins::NRESET) {
                // "open drain"
                self.nreset.into_floating_disabled();
            } else {
                self.nreset.into_push_pull_output();
                self.nreset.set_low().ok();
            }
        }

        cortex_m::asm::delay(self.cycles_per_us * wait_us);

        self.swclk.into_floating_input();
        self.swdio.into_floating_input();
        self.nreset.into_floating_input();

        let mut ret = swj::Pins::empty();
        ret.set(swj::Pins::SWCLK, matches!(self.swclk.is_high(), Ok(true)));
        ret.set(swj::Pins::SWDIO, matches!(self.swdio.is_high(), Ok(true)));
        ret.set(swj::Pins::NRESET, matches!(self.nreset.is_high(), Ok(true)));

        ret
    }

    fn sequence(&mut self, data: &[u8], mut bits: usize) {
        self.swdio.into_push_pull_output();
        self.swclk.into_push_pull_output();

        let half_period_ticks = self.half_period_ticks;

        for byte in data {
            let mut byte = *byte;
            let frame_bits = core::cmp::min(bits, 8);
            for _ in 0..frame_bits {
                let bit = byte & 1;
                byte >>= 1;
                if bit != 0 {
                    self.swdio.set_high().ok();
                } else {
                    self.swdio.set_low().ok();
                }
                self.swclk.set_low().ok();
                cortex_m::asm::delay(half_period_ticks);
                self.swclk.set_high().ok();
                cortex_m::asm::delay(half_period_ticks);
            }
            bits -= frame_bits;
        }

        self.swclk.into_floating_input();
        self.swdio.into_floating_input();
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        if max_frequency < self.cpu_frequency {
            self.max_frequency = max_frequency;
            self.half_period_ticks = self.cpu_frequency / self.max_frequency / 2;
            true
        } else {
            false
        }
    }
}

pub struct Leds {}

impl dap::DapLeds for Leds {
    fn react_to_host_status(&mut self, _host_status: dap::HostStatus) {}
}

pub struct Jtag(Context);

impl jtag::Jtag<Context> for Jtag {
    const AVAILABLE: bool = false;

    fn new(context: Context) -> Self {
        Jtag(context)
    }

    fn release(self) -> Context {
        self.0
    }

    fn sequences(&mut self, _data: &[u8], _rxbuf: &mut [u8]) -> u32 {
        0
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        self.0.set_clock(max_frequency)
    }
}

pub struct Swd(Context);

impl swd::Swd<Context> for Swd {
    const AVAILABLE: bool = true;

    fn new(mut context: Context) -> Self {
        context.swdio.into_push_pull_output();
        context.swclk.into_push_pull_output();

        Self(context)
    }

    fn release(mut self) -> Context {
        self.0.swclk.into_floating_input();
        self.0.swdio.into_floating_input();

        self.0
    }

    fn configure(&mut self, period: swd::TurnaroundPeriod, data_phase: swd::DataPhase) -> bool {
        period == swd::TurnaroundPeriod::Cycles1 && data_phase == swd::DataPhase::NoDataPhase
    }

    fn read_inner(&mut self, apndp: swd::APnDP, a: swd::DPRegister) -> swd::Result<u32> {
        // Send request
        let req = swd::make_request(apndp, swd::RnW::R, a);
        self.tx8(req);

        // Read ack, 1 clock for turnaround and 3 for ACK
        let ack = self.rx4() >> 1;
        match swd::Ack::try_ok(ack as u8) {
            Ok(_) => (),
            Err(e) => {
                // On non-OK ACK, target has released the bus but
                // is still expecting a turnaround clock before
                // the next request, and we need to take over the bus.
                self.idle_low();
                return Err(e);
            }
        }

        // Read data and parity
        let (data, parity) = self.read_data();

        // Turnaround + trailing
        self.rx8();
        self.write_bit(0); // Drive the SWDIO line to 0 to not float

        if parity as u8 == (data.count_ones() as u8 & 1) {
            Ok(data)
        } else {
            Err(swd::Error::BadParity)
        }
    }

    fn write_inner(&mut self, apndp: swd::APnDP, a: swd::DPRegister, data: u32) -> swd::Result<()> {
        // Send request
        let req = swd::make_request(apndp, swd::RnW::W, a);
        self.tx8(req);

        // Read ack, 1 clock for turnaround and 3 for ACK and 1 for turnaround
        let ack = (self.rx5() >> 1) & 0b111;
        match swd::Ack::try_ok(ack as u8) {
            Ok(_) => (),
            Err(e) => {
                // On non-OK ACK, target has released the bus but
                // is still expecting a turnaround clock before
                // the next request, and we need to take over the bus.
                self.idle_low();
                return Err(e);
            }
        }

        // Send data and parity
        let parity = data.count_ones() & 1 == 1;
        self.send_data(data, parity);

        // Send trailing idle
        self.tx8(0);

        Ok(())
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        self.0.set_clock(max_frequency)
    }
}

impl Swd {
    fn idle_low(&mut self) {
        for _ in 0..4 {
            self.write_bit(0);
        }
    }

    fn tx8(&mut self, mut data: u8) {
        self.0.swdio.into_push_pull_output();
        for _ in 0..8 {
            self.write_bit(data & 1);
            data >>= 1;
        }
    }

    fn rx4(&mut self) -> u8 {
        self.0.swdio.into_floating_input();

        let mut data = 0;

        for _ in 0..4 {
            data |= self.read_bit() & 1;
            data <<= 1;
        }

        data
    }

    fn rx5(&mut self) -> u8 {
        self.0.swdio.into_floating_input();

        let mut data = 0;

        for _ in 0..5 {
            data |= self.read_bit() & 1;
            data <<= 1;
        }

        data
    }

    fn rx8(&mut self) -> u8 {
        self.0.swdio.into_floating_input();

        let mut data = 0;

        for _ in 0..8 {
            data |= self.read_bit() & 1;
            data <<= 1;
        }

        data
    }

    fn send_data(&mut self, mut data: u32, parity: bool) {
        self.0.swdio.into_push_pull_output();

        for _ in 0..32 {
            self.write_bit((data & 1) as u8);
            data >>= 1;
        }

        self.write_bit(parity as u8);
    }

    fn read_data(&mut self) -> (u32, bool) {
        self.0.swdio.into_floating_input();

        let mut data = 0;

        for _ in 0..32 {
            data |= (self.read_bit() & 1) as u32;
            data <<= 1;
        }

        let parity = self.read_bit() != 0;

        (data, parity)
    }

    fn write_bit(&mut self, bit: u8) {
        if bit != 0 {
            self.0.swdio.set_high().ok();
        } else {
            self.0.swdio.set_low().ok();
        }
        self.0.swclk.set_low().ok();
        cortex_m::asm::delay(self.0.half_period_ticks);
        self.0.swclk.set_high().ok();
        cortex_m::asm::delay(self.0.half_period_ticks);
    }

    fn read_bit(&mut self) -> u8 {
        self.0.swclk.set_low().ok();
        cortex_m::asm::delay(self.0.half_period_ticks);
        let bit = matches!(self.0.swdio.is_high(), Ok(true)) as u8;
        self.0.swclk.set_high().ok();
        cortex_m::asm::delay(self.0.half_period_ticks);

        bit
    }
}

pub struct Swo {}

impl swo::Swo for Swo {
    fn set_transport(&mut self, _transport: swo::SwoTransport) {}

    fn set_mode(&mut self, _mode: swo::SwoMode) {}

    fn set_baudrate(&mut self, _baudrate: u32) -> u32 {
        0
    }

    fn set_control(&mut self, _control: swo::SwoControl) {}

    fn polling_data(&mut self, _buf: &mut [u8]) -> u32 {
        0
    }

    fn streaming_data(&mut self) {}

    fn is_active(&self) -> bool {
        false
    }

    fn bytes_available(&self) -> u32 {
        0
    }

    fn buffer_size(&self) -> u32 {
        0
    }

    fn support(&self) -> swo::SwoSupport {
        swo::SwoSupport {
            uart: false,
            manchester: false,
        }
    }

    fn status(&mut self) -> swo::SwoStatus {
        swo::SwoStatus {
            active: false,
            trace_error: false,
            trace_overrun: false,
            bytes_available: 0,
        }
    }
}

pub struct Wait {
    cycles_per_us: u32,
}

impl Wait {
    pub fn new(cpu_frequency: u32) -> Self {
        Wait {
            cycles_per_us: cpu_frequency / 1_000_000,
        }
    }
}

impl DelayUs<u32> for Wait {
    fn delay_us(&mut self, us: u32) {
        cortex_m::asm::delay(self.cycles_per_us * us);
    }
}

pub fn create_dap(
    version_string: &'static str,
) -> dap::Dap<'static, Context, Leds, Wait, Jtag, Swd, Swo> {
    todo!()
}