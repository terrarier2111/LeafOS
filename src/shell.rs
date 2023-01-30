use alloc::string::String;
use core::fmt;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use pc_keyboard::{DecodedKey, KeyCode};
use spin::{Mutex, MutexGuard};
use crate::arch::without_interrupts;
use crate::vga_buffer::{ColoredString, Writer};

lazy_static! {
    pub static ref SHELL: Mutex<Shell> = Mutex::new(Shell::new(ColoredString::from_string(String::from("Test: "))));
    pub static ref INITIALIZED: AtomicBool = AtomicBool::new(false);
}

pub fn has_shell() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

pub struct Shell {
    prompt: ColoredString,
    written_char_count: usize,
    prompt_enabled: bool,
}

impl Shell {

    pub fn new(prompt: ColoredString) -> Self {
        Self {
            prompt,
            written_char_count: 0,
            prompt_enabled: true,
        }
    }

    pub fn init(&mut self) {
        let mut writer = crate::vga_buffer::WRITER.lock();
        if !writer.is_current_row_clear() {
            writer.new_line();
        }
        writer.write_colored_string(&self.prompt);
        INITIALIZED.store(true, Ordering::Release);
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

    fn print_prompt(&self, writer: &mut MutexGuard<Writer>) {
        if self.prompt_enabled {
            writer.write_colored_string(&self.prompt);
        }
    }

    fn newline(&mut self, writer: &mut MutexGuard<Writer>) {
        writer.new_line();
        self.print_prompt(writer);
        self.written_char_count = 0;
    }

    pub fn write(&mut self, text: &str) {
        let mut writer = crate::vga_buffer::WRITER.lock();
        for char in text.bytes() {
            match char {
                b'\n' => {
                    self.newline(&mut writer);
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
                    if self.written_char_count > 0 {
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
                    without_interrupts(|| {
                        writer.write_fmt(format_args!("{:?}", key)).unwrap();
                    });
                    self.written_char_count += 1;
                }
            },
            DecodedKey::Unicode(key) => {
                const BACKSPACE: char = 8 as char;
                if key == BACKSPACE {
                    if self.written_char_count > 0 {
                        let mut writer = crate::vga_buffer::WRITER.lock();
                        if writer.get_column_position() > 0 {
                            let pos = writer.get_column_position();
                            writer.set_column_position(pos - 1);
                        } else {
                            writer.old_line();
                            writer.set_column_position(crate::vga_buffer::BUFFER_WIDTH - 1);
                        }
                        writer.set_byte(b' ');
                        self.written_char_count -= 1;
                    }
                } else {
                    // FIXME: Only print a-Z, 0-9
                    const ENTER: char = 10 as char;

                    let mut writer = crate::vga_buffer::WRITER.lock();
                    if key == ENTER {
                        self.newline(&mut writer);
                    } else {
                        writer.write_fmt(format_args!("{}", key)).unwrap();
                        self.written_char_count += 1;
                    }

                }
            }
        }
    }

    pub fn set_enable_prompt(&mut self, enabled: bool) {
        self.prompt_enabled = enabled;
    }

}

impl Write for Shell {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write(s);
        Ok(())
    }
}
