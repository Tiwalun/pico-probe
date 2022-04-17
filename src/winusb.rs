use usb_device::{
    class::{ControlIn, UsbClass},
    class_prelude::{BosWriter, UsbBus},
    control::RequestType,
};

const fn u16_low(val: u16) -> u8 {
    val.to_le_bytes()[0]
}

const fn u16_high(val: u16) -> u8 {
    val.to_le_bytes()[1]
}

enum MsDescriptorTypes {
    Header = 0x0,
    HeaderConfiguration = 0x1,
    HeaderFunction = 0x2,
    CompatibleId = 0x3,
    RegistryProperty = 0x4,
}

const VENDOR_ID: u8 = 0x42;

const DESCRIPTOR_SIZE: u16 = 168;

pub const DAP_V2_INTERFACE: u8 = 2;

const MS_DESCRIPTOR: [u8; DESCRIPTOR_SIZE as usize] = [
    0xa,
    0x00, // Length 10 bytes
    0x00,
    0x00, // HEADER_DESCRIPTOR
    0x00,
    0x00,
    0x03,
    0x06, // Windows version
    u16_low(DESCRIPTOR_SIZE),
    u16_high(DESCRIPTOR_SIZE),
    // Function header,
    0x8,
    0x0, // Length 8
    MsDescriptorTypes::HeaderFunction as u8,
    0x00,
    DAP_V2_INTERFACE, // First interface (dap v2 -> 1)
    0x0,              // reserved
    (DESCRIPTOR_SIZE - 0xa) as u8,
    0x00, // Subset length, including header
    // compatible ID descriptor
    20,
    0x00, // length 20
    MsDescriptorTypes::CompatibleId as u8,
    0x00,
    b'W',
    b'I',
    b'N',
    b'U',
    b'S',
    b'B',
    0x00,
    0x00, // Compatible ID: 8 bytes ASCII
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00,
    0x00, // Sub-Compatible ID: 8 bytes ASCII
    // Registry property
    78 + 2 + 42 + 2 + 2 + 2 + 2,
    0x00, // length
    MsDescriptorTypes::RegistryProperty as u8,
    0x00,
    7,
    0, // Data type: multi sz
    42,
    0x00, // property name length,
    b'D',
    0,
    b'e',
    0,
    b'v',
    0,
    b'i',
    0,
    b'c',
    0,
    b'e',
    0,
    b'I',
    0,
    b'n',
    0,
    b't',
    0,
    b'e',
    0,
    b'r',
    0,
    b'f',
    0,
    b'a',
    0,
    b'c',
    0,
    b'e',
    0,
    b'G',
    0,
    b'U',
    0,
    b'I',
    0,
    b'D',
    0,
    b's',
    0,
    0,
    0,
    78,
    0x00, // data length
    b'{',
    0,
    b'C',
    0,
    b'D',
    0,
    b'B',
    0,
    b'3',
    0,
    b'B',
    0,
    b'5',
    0,
    b'A',
    0,
    b'D',
    0,
    b'-',
    0,
    b'2',
    0,
    b'9',
    0,
    b'3',
    0,
    b'B',
    0,
    b'-',
    0,
    b'4',
    0,
    b'6',
    0,
    b'6',
    0,
    b'3',
    0,
    b'-',
    0,
    b'A',
    0,
    b'A',
    0,
    b'3',
    0,
    b'6',
    0,
    b'-',
    0,
    b'1',
    0,
    b'A',
    0,
    b'A',
    0,
    b'E',
    0,
    b'4',
    0,
    b'6',
    0,
    b'4',
    0,
    b'6',
    0,
    b'3',
    0,
    b'7',
    0,
    b'7',
    0,
    b'6',
    0,
    b'}',
    0,
    0,
    0,
];

pub struct MicrosoftDescriptors;

impl<B: UsbBus> UsbClass<B> for MicrosoftDescriptors {
    fn get_bos_descriptors(&self, writer: &mut BosWriter) -> usb_device::Result<()> {
        writer.capability(
            5,
            &[
                0,    // reserved
                0xdf, //0xdf, // wrong id to prevent Windows from using it
                0x60,
                0xdd,
                0xd8,
                0x89,
                0x45,
                0xc7,
                0x4c,
                0x9c,
                0xd2,
                0x65,
                0x9d,
                0x9e,
                0x64,
                0x8A,
                0x9f, // platform capability UUID , Microsoft OS 2.0 platform compabitility
                0x00,
                0x00,
                0x03,
                0x06, // Minimum compatible Windows version (8.1)
                u16_low(DESCRIPTOR_SIZE),
                u16_high(DESCRIPTOR_SIZE), // desciptor set total len ,
                VENDOR_ID,
                0x0, // Device does not support alternate enumeration
            ],
        )
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = xfer.request();

        if req.request_type != RequestType::Vendor {
            return;
        }

        if req.request == VENDOR_ID {
            if req.index == 7 {
                xfer.accept_with_static(&MS_DESCRIPTOR).ok();
            } else {
                xfer.reject().ok();
            }
        }
    }
}
