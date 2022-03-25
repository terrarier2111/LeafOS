pub struct Process {
    id: u64,
    state: State,
}

#[repr(u8)]
pub enum State {
    Running,
    ShuttingDown,
}