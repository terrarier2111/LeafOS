use core::arch::x86_64::{__cpuid, has_cpuid};
use crate::apic::AccessKind::{RO, RW};

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
    LVT_INITIAL_COUNT_REG, 0x3E0, RW, // for Timer
);

const IA32_APIC_BASE_MSR: usize = 0x1B;