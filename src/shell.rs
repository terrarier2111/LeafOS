use alloc::{format, vec};
use alloc::string::String;
use alloc::vec::Vec;
use core::{fmt, ptr};
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use pc_keyboard::{DecodedKey, KeyCode};
use spin::Mutex;
use x86_64::instructions::interrupts;
use crate::vga_buffer::ColoredString;

lazy_static! {
    pub static ref TESTVEC: Mutex<Vec<u8>> = Mutex::new(vec![]);
    pub static ref SHELL: Mutex<Shell> = Mutex::new(Shell::new(ColoredString::from_string(String::from("Test: "))));
}

pub static mut TEST: u64 = 0;
pub static mut TEST2: u64 = 0;

pub fn has_shell() -> bool {
    // let shell = SHELL.lock();
    // shell.is_some()
    // SHELL.lock().is_init()
    let tmp = unsafe { TEST };
    tmp == 1
}

// Mutex::new(Shell::new(ColoredString::from_string(String::from("Test: "))));

pub struct Shell {
    prompt: ColoredString,
    written_char_count: usize,
    init: bool,
}

impl Shell {

    // Uses vga_buffer char driver to check for empty current line in vga_buffer

    pub fn new(prompt: ColoredString) -> Self {
        Self {
            prompt,
            written_char_count: 0,
            init: true
        }
    }

    pub fn is_init(&self) -> bool {
        self.init
    }

    pub fn init(&mut self) {
        let mut writer = crate::vga_buffer::WRITER.lock();
        if !writer.is_current_row_clear() {
            writer.new_line();
        }
        writer.write_colored_string(&self.prompt);
        self.init = true;
    }

    pub fn write_colored(&mut self, text: &ColoredString) {
        let mut writer = crate::vga_buffer::WRITER.lock();
        for char in text.chars() {
            match char.raw_char() {
                b'\n' => {
                    writer.new_line();
                    writer.write_colored_string(&self.prompt);
                    self.written_char_count = 0;
                },
                // printable ASCII byte or newline
                0x20..=0x7e => writer.write_byte_colored(char.raw_char(), char.raw_color()),
                // not part of printable ASCII range
                _ => writer.write_byte(0xfe),
            }
        }
    }

    pub fn write(&mut self, text: &str) {
        let mut writer = crate::vga_buffer::WRITER.lock();
        for char in text.bytes() {
            match char {
                b'\n' => {
                    writer.new_line();
                    writer.write_colored_string(&self.prompt);
                    self.written_char_count = 0;
                },
                // printable ASCII byte or newline
                0x20..=0x7e => writer.write_byte(char),
                // not part of printable ASCII range
                _ => writer.write_byte(0xfe),
            }
        }
    }

    pub fn key_event(&mut self, key: DecodedKey) {
        match key {
            DecodedKey::RawKey(key) => {
                if key == KeyCode::Backspace {
                    if self.written_char_count < 0 {
                        let mut writer = crate::vga_buffer::WRITER.lock();
                        if writer.get_column_position() > 0 {
                            let pos = writer.get_column_position();
                            writer.set_column_position(pos - 1);
                            writer.write_byte(b' ');
                            let pos = writer.get_column_position();
                            writer.set_column_position(pos - 1);
                        }
                        self.written_char_count -= 1;
                    }
                } else {
                    // FIXME: Only print a-Z, 0-9
                    let mut writer = crate::vga_buffer::WRITER.lock();
                    interrupts::without_interrupts(|| {
                        writer.write_fmt(format_args!("{:?}", key));
                    });
                    self.written_char_count += 1;
                }
            },
            DecodedKey::Unicode(key) => {
                if key == char::MAX { // FIXME: Fix this!
                    if self.written_char_count < 0 {
                        let mut writer = crate::vga_buffer::WRITER.lock();
                        if writer.get_column_position() > 0 {
                            let pos = writer.get_column_position();
                            writer.set_column_position(pos - 1);
                            writer.write_byte(b' ');
                            let pos = writer.get_column_position();
                            writer.set_column_position(pos - 1);
                        }
                        self.written_char_count -= 1;
                    }
                } else {
                    // FIXME: Only print a-Z, 0-9
                    let mut writer = crate::vga_buffer::WRITER.lock();
                    interrupts::without_interrupts(|| {
                        writer.write_fmt(format_args!("{}", key as u32));
                    });
                    self.written_char_count += 1;
                }
            }
        }
    }

}

impl fmt::Write for Shell {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write(s);
        Ok(())
    }
}
