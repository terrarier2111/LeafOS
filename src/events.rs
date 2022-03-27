use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use pc_keyboard::DecodedKey;
use spin::Mutex;

lazy_static! {
    pub static ref EVENT_HANDLERS: Mutex<EventHandlers> = Mutex::new(EventHandlers::new());
}

pub struct KeyboardEvent {
    pub key: DecodedKey,
}

pub type KeyboardHandler = dyn FnMut(&KeyboardEvent) + Sync + Send;

pub struct EventHandlers {
    keyboard: Vec<Box<KeyboardHandler>>,
}

impl EventHandlers {

    pub fn new() -> Self {
        Self {
            keyboard: vec![],
        }
    }

    pub fn register_keyboard_handler(&mut self, handler: Box<KeyboardHandler>) {
        self.keyboard.push(handler);
    }

    pub fn call_keyboard_event(&mut self, event: KeyboardEvent) {
        for handler in &mut self.keyboard {
            handler(&event);
        }
    }

}
