use ssd1327_i2c::SSD1327I2C;
use tinybmp::Bmp;

use embedded_graphics::{
    image::Image,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Gray4,
    prelude::*,
    primitives::Rectangle,
};
use embedded_hal::i2c::I2c;

use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::TextBoxStyleBuilder,
    TextBox,
};

macro_rules! version_string {
    () => {
        concat!(env!("CARGO_PKG_VERSION"), "\n", env!("GIT_HASH"))
    };
}

pub enum DisplayLocation {
    L,
    R,
}

pub struct Displays<I2CL, I2CR> {
    display_l: Display<I2CL>,
    display_r: Display<I2CR>,
}

impl<I2CL: I2c, I2CR: I2c> Displays<I2CL, I2CR> {
    pub fn new(i2c_l: I2CL, i2c_r: I2CR) -> Self {
        Displays {
            display_l: Display::new(i2c_l, 0x3D),
            display_r: Display::new(i2c_r, 0x3C),
        }
    }
    pub fn splash_screen(&mut self) {
        self.display_l.splash_screen();
        self.display_r.show();
    }

    pub fn show(&mut self, loc: DisplayLocation) {
        match loc {
            DisplayLocation::L => self.display_l.show(),
            DisplayLocation::R => self.display_r.show(),
        }
    }

    pub fn draw_midi_log(&mut self, midi_log: &pedalboard_midi::display::MidiLog) {
        if let Some(display) = &mut self.display_l.driver {
            midi_log.draw(display).ok();
            display.flush().ok();
        }
    }

    /// Draw MIDI log on the right display only (for config mode).
    pub fn draw_midi_log_right(&mut self, midi_log: &pedalboard_midi::display::MidiLog) {
        if let Some(display) = &mut self.display_r.driver {
            midi_log.draw(display).ok();
            display.flush().ok();
        }
    }

    pub fn draw_performance(&mut self, preset: &pedalboard_midi::views::performance::PresetMeta) {
        use pedalboard_midi::views::performance;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            performance::draw(display, preset, performance::Side::Left).ok();
            display.flush().ok();
        }
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            performance::draw(display, preset, performance::Side::Right).ok();
            display.flush().ok();
        }
    }

    /// Redraw only the left display (buttons D, E, A).
    pub fn draw_performance_left(
        &mut self,
        preset: &pedalboard_midi::views::performance::PresetMeta,
    ) {
        use pedalboard_midi::views::performance;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            performance::draw(display, preset, performance::Side::Left).ok();
            display.flush().ok();
        }
    }

    /// Redraw only the right display (buttons F, B, C).
    pub fn draw_performance_right(
        &mut self,
        preset: &pedalboard_midi::views::performance::PresetMeta,
    ) {
        use pedalboard_midi::views::performance;
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            performance::draw(display, preset, performance::Side::Right).ok();
            display.flush().ok();
        }
    }

    /// Partial update: redraw only the rows corresponding to the changed buttons.
    /// `changed` is a 6-element array indexed by button (A=0..F=5).
    pub fn draw_performance_partial(
        &mut self,
        preset: &pedalboard_midi::views::performance::PresetMeta,
        changed: [bool; 6],
    ) {
        use pedalboard_midi::views::performance;

        // Left display: D(3)=row0, E(4)=row1, A(0)=row2
        let left_rows: [(usize, u32); 3] = [(3, 0), (4, 1), (0, 2)];
        // Right display: F(5)=row0, B(1)=row1, C(2)=row2
        let right_rows: [(usize, u32); 3] = [(5, 0), (1, 1), (2, 2)];

        if let Some(display) = &mut self.display_l.driver {
            for &(btn, row) in &left_rows {
                if changed[btn] {
                    performance::draw_row(display, preset, performance::Side::Left, row).ok();
                    // Row 2 (button A) overlaps with the preset number — redraw it.
                    if row == 2 {
                        performance::draw_preset_number(display, preset.preset_number).ok();
                    }
                    let (start, end) = performance::row_flush_range(row as usize);
                    display.flush_rows(start, end).ok();
                }
            }
        }

        if let Some(display) = &mut self.display_r.driver {
            for &(btn, row) in &right_rows {
                if changed[btn] {
                    performance::draw_row(display, preset, performance::Side::Right, row).ok();
                    let (start, end) = performance::row_flush_range(row as usize);
                    display.flush_rows(start, end).ok();
                }
            }
        }
    }

    pub fn draw_overlay(&mut self, loc: DisplayLocation, label: &str, value: u8) {
        use pedalboard_midi::views::overlay;
        match loc {
            DisplayLocation::L => {
                if let Some(display) = &mut self.display_l.driver {
                    display.clear(Gray4::BLACK).ok();
                    overlay::draw(display, label, value).ok();
                    display.flush().ok();
                }
            }
            DisplayLocation::R => {
                if let Some(display) = &mut self.display_r.driver {
                    display.clear(Gray4::BLACK).ok();
                    overlay::draw(display, label, value).ok();
                    display.flush().ok();
                }
            }
        }
    }

    pub fn draw_preset_overlay(&mut self, number: u8, name: &str, forward: bool) {
        use pedalboard_midi::views::preset_overlay;
        // Arrow on the side of the button pressed, number+name on the other
        if forward {
            // F is right → arrow on right, preset on left
            if let Some(display) = &mut self.display_l.driver {
                display.clear(Gray4::BLACK).ok();
                preset_overlay::draw(display, number, name).ok();
                display.flush().ok();
            }
            if let Some(display) = &mut self.display_r.driver {
                display.clear(Gray4::BLACK).ok();
                preset_overlay::draw_name(display, ">>").ok();
                display.flush().ok();
            }
        } else {
            // D is left → arrow on left, preset on right
            if let Some(display) = &mut self.display_l.driver {
                display.clear(Gray4::BLACK).ok();
                preset_overlay::draw_name(display, "<<").ok();
                display.flush().ok();
            }
            if let Some(display) = &mut self.display_r.driver {
                display.clear(Gray4::BLACK).ok();
                preset_overlay::draw(display, number, name).ok();
                display.flush().ok();
            }
        }
    }

    /// Show preset overlay on the left display only (right keeps performance view).
    pub fn draw_preset_overlay_left(&mut self, number: u8, name: &str, forward: bool) {
        use pedalboard_midi::views::preset_overlay;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            let arrow = if forward { ">>" } else { "<<" };
            preset_overlay::draw_with_arrow(display, number, name, arrow).ok();
            display.flush().ok();
        }
    }

    pub fn draw_long_press_hint(&mut self, label: &str) {
        use pedalboard_midi::views::preset_overlay;
        // Show hint on both displays
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            preset_overlay::draw_long_press_hint(display, label).ok();
            display.flush().ok();
        }
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            preset_overlay::draw_long_press_hint(display, label).ok();
            display.flush().ok();
        }
    }

    pub fn draw_system_status(&mut self, status: pedalboard_midi::system_status::SystemStatus) {
        let label = status.label();
        let character_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
        let textbox_style = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Middle)
            .build();
        let bounds = Rectangle::new(Point::zero(), Size::new(128, 128));

        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            TextBox::with_textbox_style(label, bounds, character_style, textbox_style)
                .draw(display)
                .ok();
            display.flush().ok();
        }
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            TextBox::with_textbox_style(label, bounds, character_style, textbox_style)
                .draw(display)
                .ok();
            display.flush().ok();
        }
    }

    /// Show a brief informational message on both displays.
    pub fn draw_message(&mut self, text: &str) {
        let character_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);
        let textbox_style = TextBoxStyleBuilder::new()
            .alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Middle)
            .build();
        let bounds = Rectangle::new(Point::zero(), Size::new(128, 128));

        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            TextBox::with_textbox_style(text, bounds, character_style, textbox_style)
                .draw(display)
                .ok();
            display.flush().ok();
        }
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            display.flush().ok();
        }
    }

    // --- Config Mode display methods ---

    pub fn draw_config_entered(&mut self) {
        use pedalboard_midi::views::config_mode;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_entered(display).ok();
            display.flush().ok();
        }
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_entered(display).ok();
            display.flush().ok();
        }
    }

    pub fn draw_config_info(&mut self, info: &pedalboard_midi::config_mode::InfoScreen) {
        use pedalboard_midi::views::config_mode;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_info_left(display, info).ok();
            display.flush().ok();
        }
        if let Some(display) = &mut self.display_r.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_info_right(display, info).ok();
            display.flush().ok();
        }
    }

    pub fn draw_config_button_press(&mut self, button: &str, detail: &str) {
        use pedalboard_midi::views::config_mode;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_button_press(display, button, detail).ok();
            display.flush().ok();
        }
    }

    pub fn draw_config_encoder_turn(&mut self, encoder: &str, detail: &str) {
        use pedalboard_midi::views::config_mode;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_encoder_turn(display, encoder, detail).ok();
            display.flush().ok();
        }
    }

    pub fn draw_config_expression(&mut self, pedal: &str, raw_adc: u16, detail: &str) {
        use pedalboard_midi::views::config_mode;
        if let Some(display) = &mut self.display_l.driver {
            display.clear(Gray4::BLACK).ok();
            config_mode::draw_expression(display, pedal, raw_adc, detail).ok();
            display.flush().ok();
        }
    }
}

struct Display<I2C> {
    driver: Option<SSD1327I2C<I2C>>,
}

impl<I2C: I2c> Display<I2C> {
    fn new(i2c: I2C, addr: u8) -> Self {
        let mut driver = ssd1327_i2c::SSD1327I2C::with_addr(i2c, addr);
        driver.init();

        Display {
            driver: Option::Some(driver),
        }
    }
    fn splash_screen(&mut self) {
        if let Some(disp) = &mut self.driver {
            let bmp_data = include_bytes!("../../../img/pedalboard-logo.bmp");

            let bmp = Bmp::from_slice(bmp_data).unwrap();

            Image::new(&bmp, Point::new(0, 0)).draw(disp).unwrap();

            disp.flush().unwrap();
        }
    }

    fn show(&mut self) {
        if let Some(display) = &mut self.driver {
            display.clear(Gray4::BLACK).unwrap();

            let text = version_string!();
            let character_style = MonoTextStyle::new(&FONT_10X20, Gray4::WHITE);

            let textbox_style = TextBoxStyleBuilder::new()
                .alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Middle)
                .paragraph_spacing(6)
                .build();

            let bounds = Rectangle::new(Point::zero(), Size::new(128, 128));
            let text_box =
                TextBox::with_textbox_style(text, bounds, character_style, textbox_style);

            text_box.draw(display).unwrap();

            display.flush().unwrap();
        }
    }
}
