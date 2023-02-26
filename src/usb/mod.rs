use midi_types::MidiMessage;
use usbd_midi::data::byte::u7::U7;
use usbd_midi::data::midi::channel::Channel;
use usbd_midi::data::midi::message::control_function::ControlFunction;
use usbd_midi::data::midi::message::Message::ControlChange;
use usbd_midi::data::midi::message::Message::ProgramChange;
use usbd_midi::data::usb_midi::cable_number::CableNumber::Cable0;
use usbd_midi::data::usb_midi::usb_midi_event_packet::UsbMidiEventPacket;

pub fn map_midi(m: midi_types::MidiMessage) -> Option<UsbMidiEventPacket> {
    match m {
        MidiMessage::ControlChange(ch, co, v) => Some(ControlChange(
            map_channel(ch),
            map_control(co),
            map_value7(v),
        )),
        MidiMessage::ProgramChange(ch, p) => Some(ProgramChange(map_channel(ch), map_program(p))),
        _ => None,
    }
    .map(|am| UsbMidiEventPacket::from_midi(Cable0, am))
}

fn map_channel(c: midi_types::Channel) -> Channel {
    let cu8: u8 = c.into();
    Channel::try_from(cu8).unwrap_or(Channel::Channel1)
}

fn map_program(p: midi_types::Program) -> U7 {
    let cu8: u8 = p.into();
    U7::try_from(cu8).unwrap_or(U7::MIN)
}

fn map_control(c: midi_types::Control) -> ControlFunction {
    let cu8: u8 = c.into();
    let cu7 = U7::try_from(cu8).unwrap_or(U7::MIN);
    ControlFunction(cu7)
}

fn map_value7(v: midi_types::Value7) -> U7 {
    let vu8: u8 = v.into();
    U7::try_from(vu8).unwrap_or(U7::MIN)
}
