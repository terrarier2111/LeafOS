use core::fmt;
use x86_64::instructions::interrupts;
use crate::shell::{has_shell, SHELL, Shell};
use crate::vga_buffer::WRITER;
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::print::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

/// Prints the given formatted string to the VGA text buffer
/// through the global `WRITER` instance.
#[inline(never)]
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    /*interrupts::without_interrupts(|| {
        if has_shell() {
            SHELL.lock().write_fmt(args);
        } else {
            WRITER.lock().write_fmt(args);
        }
    });*/
    if has_shell() {
        SHELL.lock().write_fmt(args);
    } else {
        WRITER.lock().write_fmt(args);
    }
}