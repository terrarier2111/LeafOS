pub struct Process {
    id: u64,
    pub(crate) state: State,
}

impl Process {

    pub(crate) fn new(id: u64, state: State) -> Self {
        Self {
            id,
            state
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

}

#[repr(u8)]
pub enum State {
    Waiting,
    Runnable,
    Running,
    ShuttingDown,
}