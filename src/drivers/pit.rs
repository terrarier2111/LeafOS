use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};
use raw_cpuid::CpuId;
use x86_64::instructions::interrupts::without_interrupts;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::InterruptStackFrame;
use crate::cpuid::has_cpuid;
use crate::drivers::pit::Channel::Channel0;

const CHANNEL0: u16 = 0x40; // RW
const CHANNEL1: u16 = 0x41; // RW
const CHANNEL2: u16 = 0x42; // RW
const COMMAND_REG: u16 = 0x43; // WO

// COMMAND REG:
/*
Bits         Usage
6 and 7      Select channel :
                0 0 = Channel 0
                0 1 = Channel 1
                1 0 = Channel 2
                1 1 = Read-back command
4 and 5      Access mode :
                0 0 = Latch count value command
                0 1 = Access mode: lobyte only
                1 0 = Access mode: hibyte only
                1 1 = Access mode: lobyte/hibyte
1 to 3       Operating mode :
                0 0 0 = Mode 0 (interrupt on terminal count)
                0 0 1 = Mode 1 (hardware re-triggerable one-shot)
                0 1 0 = Mode 2 (rate generator)
                0 1 1 = Mode 3 (square wave generator)
                1 0 0 = Mode 4 (software triggered strobe)
                1 0 1 = Mode 5 (hardware triggered strobe)
                1 1 0 = Mode 2 (rate generator, same as 010b)
                1 1 1 = Mode 3 (square wave generator, same as 011b)
0            BCD/Binary mode: 0 = 16-bit binary, 1 = four-digit BCD
*/

#[repr(u8)]
enum Channel {
    Channel0 = 0b00,
    Channel1 = 0b01,
    Channel2 = 0b10,
    ReadBackCommand = 0b11,
}

#[repr(u8)]
enum AccessMode {
    LatchCountDownValueCommand = 0b00,
    LoByteOnly = 0b01,
    HiByteOnly = 0b10,
    LoHiByte = 0b11,
}

#[repr(u8)]
enum OperatingMode {
    InterruptOnTerminalCount = 0b000,
    HardwareReTriggerableOneShot = 0b001,
    RateGenerator = 0b010, // alternative: 0b110
    SquareWaveGenerator = 0b011, // alternative: 0b011
    SoftwareTriggeredStrobe = 0b100,
    HardwareTriggeredStrobe = 0b101,
}

#[repr(u8)]
enum DataMode {
    Binary = 0, // 16-bit binary
    BCD = 1,    // four-digit BCD
}

fn write_mode(channel: Channel, access_mode: AccessMode, operating_mode: OperatingMode, data_mode: DataMode) {
    without_interrupts(|| {
        let mut port = Port::new(COMMAND_REG);
        let data = data_mode as u8 | ((operating_mode as u8) << 1) | ((access_mode as u8) << 4) | ((channel as u8) << 6);
        unsafe { port.write(data); }
    })
}

pub fn read_pit_count() -> u16 {
    without_interrupts(|| {
        let mut port = Port::new(COMMAND_REG);
        unsafe { port.write(0_u8); }
        let mut port = Port::new(CHANNEL0);
        let count_low: u8 = unsafe { port.read() }; // Low byte
        let count_high: u8 = unsafe { port.read() };      // High byte
        (count_low as u16) | ((count_high as u16) << 8)
    })
}

fn set_pit_count(count: u16) {
    without_interrupts(|| {
        let mut port = Port::new(CHANNEL0);
        unsafe {
            port.write((count & 0xff) as u8);          // Low byte
            port.write(((count & 0xff00) >> 8) as u8); // High byte
        }
    })
}

pub fn init() {
    set_frequency(PIT_FREQUENCY_HZ);
}

/*
struct TimerInitResult {
    frequency: u32,
    fractions: u32,
    ms: u32,
}

// reload value e.g: 0x10000
unsafe fn got_reload_value(value: u32) -> TimerInitResult {
    // let result = 3579545 / value;
    /*
    let result;
    let remainder;
    asm!("mov eax,3579545",
    "mov edx,0",                         // edx:eax = 3579545
    "div ebx",                           // eax = 3579545 / reload_value, edx = remainder
    // "cmp edx,3579545 / 2",               // Is the remainder more than half?
    out("eax") result,
    inout("ebx") value,
    out("edx") remainder,
    );
    if remainder > 3579545 / 2 {
        result += 1;
    }
    asm!("mov ebx,3",
    "mov edx,0",                         // edx:eax = 3579545 * 256 / frequency
    "div ebx",                           // eax = (3579545 * 256 / 3 * 256) / frequency
    // "cmp edx,3 / 2",                      // Is the remainder more than half?"
    inout("eax") result,
    out("ebx") value,
    out("edx") _,
    );
    if value > 3 / 2 {

    }*/
    let mut irq0_frequency = 3579545 / value;
    if (3579545 % value) > (3579545 / 2) { // FIXME: check direction
        irq0_frequency += 1;
    }
    let carry = if (irq0_frequency % 3) > (3 / 2) { // FIXME: check direction
        1
    } else {
        0
    };
    irq0_frequency /= 3;
    irq0_frequency += carry;
    // we got the IRQ0 frequency

    // let mut tmp = 0xDBB3A062 * value;
    let irq0_fractions: u32;
    let irq0_ms: u32;
    asm!(
    "mov eax, 0xDBB3A062", // magic value comes from: 3000 * (2^42) / 3579545
    "mul {}",
    "shrd eax, edx, 10",
    "shr edx, 10",
    in(reg) value,
    out("eax") irq0_fractions,
    out("edx") irq0_ms,
    );

    without_interrupts(|| {
        write_mode(Channel0, AccessMode::LoHiByte, OperatingMode::RateGenerator, DataMode::Binary);
    });
    set_pit_count(value as u16);
    TimerInitResult {
        frequency: irq0_frequency,
        fractions: irq0_fractions,
        ms: irq0_ms,
    }
}

unsafe fn init_pit(pit_freq: u32) -> TimerInitResult {
    if pit_freq <= 18 { // FIXME: check direction | jbe
        return got_reload_value(0x10000);
    }
    if pit_freq >= 1193181 { // FIXME: check direction | jae
        return got_reload_value(pit_freq);
    }
    let mut value = 3579545 / pit_freq;
    if 3579545 % pit_freq <= 3579545 / 2 { // FIXME: check direction | (inverse) jb
        value += 1;
    }
    let remainder = value % 3;
    value /= 3;
    if remainder > 3 / 2 { // FIXME: check direction | jb
        value += 1;
    }
    got_reload_value(value)
}*/

const PIT_FREQUENCY_HZ: usize = 1000;
pub const PIT_DIVIDEND: usize = 1193182;

fn set_frequency(frequency: usize) {
    let mut new_divisor = PIT_DIVIDEND / frequency;

    if PIT_DIVIDEND % frequency > frequency / 2 {
        new_divisor += 1;
    }

    write_mode(Channel0, AccessMode::LoHiByte, OperatingMode::RateGenerator, DataMode::Binary);
    set_pit_count(new_divisor as u16)
}

pub fn write_channel0_count(count: u16) {
    write_mode(Channel0, AccessMode::LoHiByte, OperatingMode::RateGenerator, DataMode::Binary);
    set_pit_count(count)
}

static COUNTDOWN: AtomicU64 = AtomicU64::new(0);

/// SAFETY:
/// The caller has to ensure that no other countdown is currently running
/// furthermore the caller has to ensure that the PIT is initialized
unsafe fn sleep(millis: u64) {
    COUNTDOWN.store(millis, Ordering::SeqCst);
    loop {
        let curr = without_interrupts(|| {
            COUNTDOWN.load(Ordering::SeqCst)
        });
        if curr == 0 {
            break;
        }
        // nop a few times so the interrupt can get handled
        asm!(
        "nop",
        "nop",
        "nop",
        "nop",
        "nop",
        "nop",
        );
    }
}

pub fn handle_timer() {
    // if has_shell() {
    // println!(".");
    // }
    let curr = COUNTDOWN.load(Ordering::SeqCst);
    if curr != 0 {
        COUNTDOWN.store(curr - 1, Ordering::SeqCst);
    }
}

// FIXME: Finish this implementation with the help from: https://wiki.osdev.org/Programmable_Interval_Timer
