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

    // FIXME: should the signaling methods even be in here?
    pub fn send_signal(&self, signal_id: u32) { // FIXME: should we use usize/u64?
        // FIXME: Call signal handler!
    }

    /// Send a real-time signal
    pub fn send_signal_rt(&self, signal_id: u32, payload: u32) { // FIXME: should we use usize/u64?
        todo!()
    }

}

#[repr(u8)]
pub enum State {
    Waiting,
    Runnable,
    Running,
    ShuttingDown,
}