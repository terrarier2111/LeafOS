use core::arch::x86_64::__cpuid;
use core::ops::Range;
use x86_64::registers::model_specific::Msr;
use crate::drivers::apic::AccessKind::{RO, RW};
use bit_field::BitField;
use lazy_static::lazy_static;
use crate::cpuid::has_cpuid;

/// Checks for presence of local APIC
pub fn is_available() -> bool {
    if !has_cpuid() {
        return false;
    }
    const APIC_ID: u32 = 1 << 8; // 9th bit of edx register
    return unsafe { __cpuid(1) }.edx & APIC_ID != 0;
}

enum AccessKind {
    RO, // read only
    WO, // write only
    RW, // read/write
}

macro_rules! define_consts {
    ($($name: ident, $val: expr, $access: expr), *) => {
        $(
                const $name: (usize, AccessKind) = ($val, $access);
            )*
    };
}

// local APIC register address map
// const LOCAL_APIC_ID_REG: usize = 0x20; // R/W
// const LOCAL_APIC_VERSION_REG: usize = 0x30; // RO
// const TASK_PRIORITY_REGISTER: usize;

define_consts!(
    LOCAL_APIC_ID_REG, 0x20, RW,
    LOCAL_APIC_VERSION_REG, 0x30, RO,
    TASK_PRIORITY_REG, 0x80, RW,
    ARBITRATION_PRIORITY_REG, 0x90, RO,
    PROCESSOR_PRIORITY_REG, 0xA0, RO,
    EOI_REG, 0xB0, RW,
    REMOTE_READ_REG, 0xC0, RO, // beware: this might not be available everywhere
    LOGICAL_DESTINATION_REG, 0xD0, RW,
    DESTINATION_FORMAT_REG, 0xE0, RW, // Read/Write (see Section 10.6.2.2)
    SPURIOUS_INTERRUPT_VECTOR_REG, 0xF0, RW, // Read/Write (see Section 10.9)

    IN_SERVICE_REG_0, 0x100, RO,
    IN_SERVICE_REG_1, 0x110, RO,
    IN_SERVICE_REG_2, 0x120, RO,
    IN_SERVICE_REG_3, 0x130, RO,
    IN_SERVICE_REG_4, 0x140, RO,
    IN_SERVICE_REG_5, 0x150, RO,
    IN_SERVICE_REG_6, 0x160, RO,
    IN_SERVICE_REG_7, 0x170, RO,

    TRIGGER_MODE_REG_0, 0x180, RO,
    TRIGGER_MODE_REG_1, 0x190, RO,
    TRIGGER_MODE_REG_2, 0x1A0, RO,
    TRIGGER_MODE_REG_3, 0x1B0, RO,
    TRIGGER_MODE_REG_4, 0x1C0, RO,
    TRIGGER_MODE_REG_5, 0x1D0, RO,
    TRIGGER_MODE_REG_6, 0x1E0, RO,
    TRIGGER_MODE_REG_7, 0x1F0, RO,

    INTERRUPT_REQUEST_REG_0, 0x200, RO,
    INTERRUPT_REQUEST_REG_1, 0x210, RO,
    INTERRUPT_REQUEST_REG_2, 0x220, RO,
    INTERRUPT_REQUEST_REG_3, 0x230, RO,
    INTERRUPT_REQUEST_REG_4, 0x240, RO,
    INTERRUPT_REQUEST_REG_5, 0x250, RO,
    INTERRUPT_REQUEST_REG_6, 0x260, RO,
    INTERRUPT_REQUEST_REG_7, 0x270, RO,

    ERROR_STATUS_REG, 0x280, RO,
    LVT_CMCI_REG, 0x2F0, RW,
    INTERRUPT_COMMAND_REG_0, 0x300, RW,
    INTERRUPT_COMMAND_REG_1, 0x310, RW,
    LVT_TIMER_REG, 0x320, RW,
    LVT_THERMAL_SENSOR_REG, 0x330, RW, // implementation dependant, try to avoid using this unless you know what you are doing!
    LVT_PERFORMANCE_MONITORING_COUNTERS_REG, 0x340, RW, // implementation dependant, try to avoid using this unless you know what you are doing!
    LVT_LINT0_REG, 0x350, RW,
    LVT_LINT1_REG, 0x360, RW,
    LVT_ERROR_REG, 0x370, RW,
    LVT_INITIAL_COUNT_REG, 0x380, RW, // for Timer
    LVT_CURRENT_COUNT_REG, 0x390, RO, // for Timer
    LVT_DIVIDE_CONFIG_REG, 0x3E0, RW // for Timer
);

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const BSP_FLAG: u64 = 1 << 7; // 8th bit (bootstrap processor flag)
const X2_APIC_MODE_ENABLE_FLAG: u64 = 1 << 9; // 10th bit
const X_APIC_GLOBAL_ENABLE_FLAG: u64 = 1 << 10; // 11th bit
const APIC_BASE_FIELD_START: u64 = 12; // 12th bit
const APIC_BASE_FIELD_END: u64 = 35; // 35th bit

unsafe fn is_bsp() -> bool {
    let mut reg = Msr::new(IA32_APIC_BASE_MSR);
    reg.read() & BSP_FLAG != 0
}

unsafe fn is_enabled() -> bool {
    let mut reg = Msr::new(IA32_APIC_BASE_MSR);
    reg.read() & X_APIC_GLOBAL_ENABLE_FLAG != 0
}

unsafe fn enable(mode: XAPICMode) {
    let mut reg = Msr::new(IA32_APIC_BASE_MSR);
    let mut tmp = reg.read() | X_APIC_GLOBAL_ENABLE_FLAG;
    if mode == XAPICMode::X2APIC {
        tmp |= X2_APIC_MODE_ENABLE_FLAG;
    }
    reg.write(tmp);
}

#[derive(PartialEq)]
enum XAPICMode {
    XAPIC,
    X2APIC,
}

/// returns the 24 bit base field (shifted by 11 bits)
unsafe fn get_base_field() -> u64 {
    // FIXME: Check this set_bits call!
    // const FIELD_RANGE: u64 = *0_u64.set_bits(0..23, 1) << 11;

    // FIXME: Check this set_bit_range_def call!
    let field_range = set_bit_range_def::<u64>(0..24) << 11; // all bits up to (including) the 24th bit should be set

    let mut reg = Msr::new(IA32_APIC_BASE_MSR);
    reg.read() & field_range
}

/// provide a 24 bit address value when calling
///
/// this function sets the base address field to
/// the address value which was passed into it
unsafe fn set_base_field(address: u32) {
    // FIXME: Check this set_bits call!
    // const FIELD_RANGE: u32 = *0_u32.set_bits(0..23, 1); // all bits up to (including) the 24th bit should be set
    // const FIELD_CLEAR_RANGE: u64 = !((FIELD_RANGE as u64) << 11);
    // FIXME: Check this set_bit_range_def call!
    let field_range = set_bit_range_def::<u32>(0..24); // all bits up to (including) the 24th bit should be set
    let field_clear_range = !((field_range as u64) << 11);

    let address = ((address & field_range) as u64) << 11;

    let mut reg = Msr::new(IA32_APIC_BASE_MSR);
    reg.write(reg.read() & field_clear_range | address);
}

const APIC_DEFAULT_ADDR: u64 = 0xFEE00000;
const LVT_CMCI_REG_OFFSET: u64 = 0x2F0;
const LVT_TIMER_REG_OFFSET: u64 = 0x320;
const LVT_THERMAL_MONITOR_REG_OFFSET: u64 = 0x330;
const LVT_PERFORMANCE_COUNTER_REG_OFFSET: u64 = 0x340;
const LVT_LI_NT0_REG_OFFSET: u64 = 0x350;
const LVT_LI_NT1_REG_OFFSET: u64 = 0x360;
const LVT_ERROR_REG_OFFSET: u64 = 0x370;

#[repr(u8)]
enum DeliveryMode {
    Fixed = 0b000,
    SMI = 0b010,
    NMI = 0b100,
    INIT = 0b101,
    ExtlNT = 0b111,
}

#[repr(u8)]
enum TimerMode {
    OneShot = 0b00,  // using a count-down value
    Periodic = 0b01, // reloading a count-down value
    TSCDeadline = 0b10, // using absolute target value in IA32_TSC_DEADLINE MSR
}

#[repr(u8)]
enum TriggerMode {
    Edge = 0,
    Level = 1,
}

#[repr(u8)]
enum DeliveryStatus {
    Idle = 0,
    SendPending = 1,
}

#[repr(transparent)]
struct Timer(u32);
// bits 31-19 | reserved
// bits 18-17 | timer mode
// bits 16-16 | mask (handled differently on some processors)
// bits 15-13 | reserved
// bits 12-12 | delivery status | RO
// bits 11-8  | reserved
// bits 7-0   | vector

impl Timer {

    fn new(mode: TimerMode) -> Self {
        todo!()
    }

}

// CMCI:
// bits 31-17 | reserved
// bits 16-16 | mask (handled differently on some processors)
// bits 15-13 | reserved
// bits 12-12 | delivery status | RO
// bits 11-11 | reserved
// bits 10-8  | delivery mode
// bits 7-0   | vector


// LINT0:
// bits 31-17 | reserved
// bits 16-16 | mask (handled differently on some processors)
// bits 15-15 | trigger mode
// bits 14-14 | remote IRR | RO
// bits 13-13 | Interrupt Input Pin Polarity
// bits 12-12 | delivery status | RO
// bits 11-11 | reserved
// bits 10-8  | delivery mode
// bits 7-0   | vector // range of 16 to 255


// LINT1:
// bits 31-17 | reserved
// bits 16-16 | mask (handled differently on some processors)
// bits 15-15 | trigger mode
// bits 14-14 | remote IRR | RO
// bits 13-13 | Interrupt Input Pin Polarity
// bits 12-12 | delivery status | RO
// bits 11-11 | reserved
// bits 10-8  | delivery mode
// bits 7-0   | vector // range of 16 to 255


// ERROR:
// bits 31-17 | reserved
// bits 16-16 | mask (handled differently on some processors)
// bits 15-13 | reserved
// bits 12-12 | delivery status | RO
// bits 11-8 | reserved
// bits 7-0   | vector // range of 16 to 255


// PERFORMANCE MON. COUNTERS:
// bits 31-17 | reserved
// bits 16-16 | mask (handled differently on some processors)
// bits 15-13 | reserved
// bits 12-12 | delivery status | RO
// bits 11-11 | reserved
// bits 10-8  | delivery mode
// bits 7-0   | vector // range of 16 to 255


// THERMAL SENSOR:
// bits 31-17 | reserved
// bits 16-16 | mask (handled differently on some processors)
// bits 15-13 | reserved
// bits 12-12 | delivery status | RO
// bits 11-11 | reserved
// bits 10-8  | delivery mode
// bits 7-0   | vector // range of 16 to 255


// ERROR HANDLING: // FIXME: Write to this before reading
// bits 31-8 | reserved
// bits 7-7  | illegal register address (processor specific)
// bits 6-6  | received illegal vector
// bits 5-5  | send illegal vector
// bits 4-4  | redirectable IPI (processor specific)
// bits 3-3  | receive accept error (processor specific)
// bits 2-2  | send accept error (processor specific)
// bits 1-1  | receive checksum error (processor specific)
// bits 0-0  | send checksum error (processor specific)


enum DivideValue {
    One = 0b111,
    Two = 0b000,
    Four = 0b001,
    Eight = 0b010,
    SixTeen = 0b011,
    ThirtyTwo = 0b100,
    SixtyFour = 0b101,
    OneHundredTwentyEight = 0b110,
}

fn setup_timer(initial_count: u32, divide_value: DivideValue) {

}

fn get_current_timer_count() -> u32 {
    // let tmp = Msr::new(get_base_field());
    let tmp = Msr::new(IA32_APIC_BASE_MSR);

    todo!()
}


#[inline]
fn set_bit_range_def<T: Default + core::ops::BitOrAssign + core::iter::Step + From<u8> + core::ops::Shl<Output = T>>(range: Range<T>) -> T {
    set_bit_range::<T>(T::default(), range)
}

#[inline]
fn set_bit_range<T: core::ops::BitOrAssign + core::iter::Step + From<u8> + core::ops::Shl<Output = T>>(base: T, range: Range<T>) -> T {
    let mut result = base;
    for i in range.start..range.end {
        let bit = T::from(1) << i;
        result |= bit;
    }
    result
}