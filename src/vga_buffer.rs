use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use volatile::Volatile;
use lazy_static::lazy_static;
use spin::Mutex;
use core::fmt;
use x86_64::instructions::interrupts;
use x86_64::structures::idt::InterruptDescriptorTable;
use crate::drivers::driver::{CharDriverImpl, Driver};
use crate::println;

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub fn new(foreground: Color, background: Color) -> Self {
        Self((background as u8) << 4 | (foreground as u8))
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

impl ScreenChar {

    pub fn new(ascii_character: u8, color_code: ColorCode) -> Self {
        Self {
            ascii_character,
            color_code,
        }
    }

    #[inline]
    pub fn raw_char(&self) -> u8 {
        self.ascii_character
    }

    #[inline]
    pub fn raw_color(&self) -> ColorCode {
        self.color_code
    }

}

pub struct ColoredString {
    chars: Vec<ScreenChar>,
    curr_color: ColorCode,
}

impl ColoredString {

    pub fn new() -> Self {
        Self {
            chars: vec![],
            curr_color: ColorCode::new(Color::White, Color::Black),
        }
    }

    pub fn from_string(str: String) -> Self {
        let mut ret = Self {
            chars: vec![],
            curr_color: ColorCode::new(Color::White, Color::Black),
        };
        for char in str.bytes() {
            ret.push_char(char);
        }
        ret
    }

    pub fn push_char(&mut self, char: u8) {
        self.chars.push(ScreenChar {
            ascii_character: char,
            color_code: self.curr_color,
        })
    }

    #[inline]
    pub fn push_colored(&mut self, char: u8, color: ColorCode) {
        self.color(color);
        self.push_char(char);
    }

    #[inline]
    pub fn color(&mut self, color: ColorCode) {
        self.curr_color = color;
    }

    #[inline]
    pub fn chars(&self) -> &Vec<ScreenChar> {
        &self.chars
    }

}

const BUFFER_HEIGHT: usize = 25;
pub const BUFFER_WIDTH: usize = 80;

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Writer {
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

impl Writer {
    pub fn write_string(&mut self, s: &str) {
        for char in s.bytes() {
            match char {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(char),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn write_colored_string(&mut self, colored: &ColoredString) {
        for char in &colored.chars {
            if self.column_position >= BUFFER_WIDTH {
                self.new_line();
            }

            let row = BUFFER_HEIGHT - 1;
            let col = self.column_position;

            self.buffer.chars[row][col].write(*char);
            self.column_position += 1;
        }
    }

    pub fn write_byte(&mut self, char: u8) {
        self.write_byte_colored(char, self.color_code);
    }

    pub fn write_byte_colored(&mut self, char: u8, color: ColorCode) {
        if !self.set_byte_colored(char, color) {
            self.column_position += 1;
        }
    }

    pub fn set_byte(&mut self, char: u8) {
        self.set_byte_colored(char, self.color_code);
    }

    pub fn set_byte_colored(&mut self, char: u8, color: ColorCode) -> bool {
        match char {
            b'\n' => {
                self.new_line();
                true
            },
            char => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position;

                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: char,
                    color_code: color,
                });
                false
            }
        }
    }

    pub fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    pub fn old_line(&mut self) {
        for row in (0..(BUFFER_HEIGHT - 1)).rev() {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row + 1][col].write(character);
            }
        }
        self.clear_row(0);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn is_current_row_clear(&self) -> bool {
        for col in 0..BUFFER_WIDTH {
            let character: ScreenChar = self.buffer.chars[BUFFER_HEIGHT - 1][col].read();
            if character.ascii_character != b' ' || character.color_code != self.color_code {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn get_column_position(&self) -> usize {
        self.column_position
    }

    #[inline]
    pub fn set_column_position(&mut self, column_pos: usize) {
        self.column_position = column_pos;
    }

}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

unsafe impl Driver for Writer {
    #[inline]
    unsafe fn init(&mut self, _idt: &mut InterruptDescriptorTable) -> bool {
        // We don't have to do anything here
        true
    }

    #[inline]
    unsafe fn exit(&mut self) {
        // We don't have to do anything here
    }
}

unsafe impl CharDriverImpl<ScreenChar> for Writer {
    unsafe fn write_char(&mut self, char: &ScreenChar) {
        self.write_byte_colored(char.ascii_character, char.color_code)
    }

    /// The format of the index parameter is the following
    /// First  byte: value from 0-24
    /// Second byte: value from 0-79
    unsafe fn write_char_indexed(&mut self, index: usize, char: &ScreenChar) {
        const HEIGHT_MASK: usize = {
            let mut start = u8::MAX as usize;
            // keep all bits up to including the 5th - drop the ones thereafter
            start &= !(1 << 5);
            start &= !(1 << 6);
            start &= !(1 << 7);
            start
        };
        const WIDTH_MASK: usize = {
            // keep all bits except the 8th one
            let mut start = u8::MAX as usize;
            start &= !(1 << 7);
            // shift the bits to their position
            start = start << 8;
            start
        };
        self.buffer.chars[index & HEIGHT_MASK][index & WIDTH_MASK] = Volatile::new(*char);
    }

    #[cold]
    unsafe fn read_char(&mut self) -> ScreenChar {
        unimplemented!()
    }

    #[cold]
    unsafe fn read_char_indexed(&mut self, _index: usize) -> ScreenChar {
        unimplemented!()
    }
}

#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}

#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        println!("test_println_many output");
    }
}

#[test_case]
fn test_println_output() {
    let s = "Some test string that fits on a single line";
    println!("{}", s);
    for (i, c) in s.chars().enumerate() {
        let screen_char = WRITER.lock().buffer.chars[BUFFER_HEIGHT - 2][i].read();
        assert_eq!(char::from(screen_char.ascii_character), c);
    }
}