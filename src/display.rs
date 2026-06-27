use core::fmt::Write;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X9, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::Rectangle,
};
use embedded_text::{alignment::HorizontalAlignment, style::TextBoxStyleBuilder, TextBox};
use heapless::String;

const DISPLAY_SIZE: u32 = 128;
const MAX_LINES: usize = 12;
const LINE_LEN: usize = 22;

/// A scrolling log of MIDI messages for display
pub struct MidiLog {
    lines: [String<LINE_LEN>; MAX_LINES],
    next: usize,
}

impl MidiLog {
    pub fn new() -> Self {
        MidiLog {
            lines: core::array::from_fn(|_| String::new()),
            next: 0,
        }
    }

    pub fn push_note_on(&mut self, ch: u8, note: u8, vel: u8) {
        let line = &mut self.lines[self.next % MAX_LINES];
        line.clear();
        write!(line, "NOn  C{:02} #{:3} v{:3}", ch, note, vel).ok();
        self.next += 1;
    }

    pub fn push_note_off(&mut self, ch: u8, note: u8) {
        let line = &mut self.lines[self.next % MAX_LINES];
        line.clear();
        write!(line, "NOff C{:02} #{:3}", ch, note).ok();
        self.next += 1;
    }

    pub fn push_cc(&mut self, ch: u8, cc: u8, val: u8) {
        let line = &mut self.lines[self.next % MAX_LINES];
        line.clear();
        write!(line, "CC   C{:02} #{:3} v{:3}", ch, cc, val).ok();
        self.next += 1;
    }

    /// Render the log to a display
    pub fn draw<D: DrawTarget<Color = Gray4>>(&self, display: &mut D) -> Result<(), D::Error> {
        display.fill_solid(
            &Rectangle::new(Point::zero(), Size::new(DISPLAY_SIZE, DISPLAY_SIZE)),
            Gray4::BLACK,
        )?;

        let style = MonoTextStyle::new(&FONT_6X9, Gray4::WHITE);
        let textbox_style = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Left)
            .build();

        let mut buf: String<256> = String::new();
        let start = if self.next >= MAX_LINES { self.next } else { 0 };

        for i in 0..MAX_LINES {
            let idx = (start + i) % MAX_LINES;
            if !self.lines[idx].is_empty() {
                writeln!(buf, "{}", self.lines[idx]).ok();
            }
        }

        let bounds = Rectangle::new(
            Point::new(2, 2),
            Size::new(DISPLAY_SIZE - 4, DISPLAY_SIZE - 4),
        );
        TextBox::with_textbox_style(buf.as_str(), bounds, style, textbox_style).draw(display)?;

        Ok(())
    }
}

impl Default for MidiLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_messages() {
        let mut log = MidiLog::new();
        log.push_note_on(1, 60, 127);
        log.push_cc(1, 7, 64);
        log.push_note_off(1, 60);
        assert_eq!(log.lines[0].as_str(), "NOn  C01 # 60 v127");
        assert_eq!(log.lines[1].as_str(), "CC   C01 #  7 v 64");
        assert_eq!(log.lines[2].as_str(), "NOff C01 # 60");
    }

    #[test]
    fn test_wraps_around() {
        let mut log = MidiLog::new();
        for i in 0..15 {
            log.push_note_on(1, i, 100);
        }
        // Should have wrapped, oldest messages overwritten
        assert_eq!(log.next, 15);
        // Slot 0 was written at iteration 0 and 12 (0 + 12 = 12, 12 % 12 = 0)
        assert!(log.lines[0].as_str().contains("# 12"));
    }
}
