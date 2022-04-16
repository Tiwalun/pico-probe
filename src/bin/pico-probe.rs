#![no_std]
#![no_main]

use pico_probe as _;

#[rtic::app(device = rp2040_hal::pac, dispatchers = [XIP_IRQ])]
mod app {
    use core::mem::MaybeUninit;
    use defmt::*;
    use embedded_hal::adc::OneShot;
    use embedded_hal::digital::v2::ToggleableOutputPin;
    use pico_probe::setup::*;
    use rp2040_hal::usb::UsbBus;
    use rp2040_monotonic::*;
    use usb_device::class_prelude::*;

    #[monotonic(binds = TIMER_IRQ_0, default = true)]
    type Monotonic = Rp2040Monotonic;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        probe_usb: pico_probe::usb::ProbeUsb,
        dap_handler: DapHandler,
        led: LedPin,
        adc: AdcReader,
    }

    #[init(local = [
        usb_bus: MaybeUninit<UsbBusAllocator<UsbBus>> = MaybeUninit::uninit(),
        delay: MaybeUninit<pico_probe::systick_delay::Delay> = MaybeUninit::uninit(),
    ])]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        let (mono, led, probe_usb, dap_handler, adc) =
            setup(cx.device, cx.core, cx.local.usb_bus, cx.local.delay);

        led_blinker::spawn().ok();

        (
            Shared {},
            Local {
                probe_usb,
                dap_handler,
                led,
                adc,
            },
            init::Monotonics(mono),
        )
    }

    #[task(local = [led, adc])]
    fn led_blinker(cx: led_blinker::Context) {
        cx.local.led.toggle().ok();
        let val = cx.local.adc.voltage();
        defmt::info!("Vtgt = {} mV", val);
        led_blinker::spawn_after(500.millis()).ok();
    }

    #[task(binds = USBCTRL_IRQ, local = [probe_usb, dap_handler, resp_buf: [u8; 64] = [0; 64]])]
    fn on_usb(ctx: on_usb::Context) {
        let probe_usb = ctx.local.probe_usb;
        let dap = ctx.local.dap_handler;
        let resp_buf = ctx.local.resp_buf;

        if let Some(request) = probe_usb.interrupt() {
            use dap_rs::{dap::DapVersion, usb::Request};

            match request {
                Request::DAP1Command((report, n)) => {
                    /*
                    let len = dap.process_command(&report[..n], resp_buf, DapVersion::V1);

                    if len > 0 {
                        probe_usb.dap1_reply(&resp_buf[..len]);
                    }
                    */
                }
                Request::DAP2Command((report, n)) => {
                    let len = dap.process_command(&report[..n], resp_buf, DapVersion::V2);

                    if len > 0 {
                        probe_usb.dap2_reply(&resp_buf[..len]);
                    }
                }
                Request::Suspend => {
                    info!("Got USB suspend command");
                    dap.suspend();
                }
            }
        }
    }
}
