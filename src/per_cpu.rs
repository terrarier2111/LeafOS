macro_rules! per_cpu {
    ($visibility:vis, $name:ident, $type:ty, $initial:expr) => {
        #[link_section = ".per_cpu"]
        $visibility static $name: $type = $initial;
    };
}

// extern static SECTION_SIZE: usize;
