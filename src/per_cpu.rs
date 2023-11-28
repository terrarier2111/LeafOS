use crate::sc_cell::SCCell;

macro_rules! per_cpu {
    ($visibility:vis, $name:ident, $type:ty, $initial:expr) => {
        #[link_section = ".per_cpu"]
        $visibility static $name: $type = $initial;
    };
}

static CORES: SCCell<usize> = SCCell::new(0);

// extern static SECTION_SIZE: usize;

pub fn setup(cores: usize) {
    unsafe { CORES.set(cores); }
}
