use crate::gdt::{KERNEL_CODE_SEGMENT_IDX, USER_CODE_SEGMENT_IDX};
use crate::process::{Process, State};
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::intrinsics::unlikely;
use core::mem::{MaybeUninit, size_of};
use core::ptr;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};
use lazy_static::lazy_static;
use spin::{Mutex, Once};
use crate::{mem, println, wait_for_interrupt};

static IDLE_TASK: Once<Arc<Mutex<(Process, ProcessState)>>> = Once::new(); // FIXME: we should probably make this per-core and MAYBE we should make this MaybeUninit and init it at startup instead of using Once!
static INIT: AtomicBool = AtomicBool::new(false); // FIXME: Make this per-core.
static mut VOID_TASK: MaybeUninit<ProcessState> = MaybeUninit::uninit();

lazy_static! {
    static ref SCHEDULER: Arc<Mutex<Box<dyn Scheduler + Send>>> = {
        Arc::new(Mutex::new(Box::new(RoundRobinScheduler::new())))
    };
}

pub const SCHEDULER_TIMER_DELAY: usize = 1000000;

pub trait Scheduler {
    // this is for internal use only
    fn pick_next(&mut self) -> Option<(Process, ProcessState)>;

    // this is for internal use only
    fn reinsert_task(&mut self, task: (Process, ProcessState));

    /// This should return different values for different cpu cores
    // fn current_process(&self) -> Option<&SchedulerEntry>;

    fn start_process(&mut self, target_fn: fn(), kernel_owned: bool) -> u64;
}

struct RoundRobinScheduler {
    tasks: Vec<(Process, ProcessState)>,
    task_id: u64,
}

impl RoundRobinScheduler {
    fn new() -> Self {
        Self {
            tasks: vec![],
            task_id: 0,
        }
    }
}

impl Scheduler for RoundRobinScheduler {
    fn pick_next(&mut self) -> Option<(Process, ProcessState)> {
        self.tasks.pop()
    }

    fn reinsert_task(&mut self, task: (Process, ProcessState)) {
        self.tasks.insert(0, task);
    }

    /*
    fn current_process(&self) -> Option<&SchedulerEntry> {
        todo!()
    }*/

    fn start_process(&mut self, target_fn: fn(), kernel_owned: bool) -> u64 {
        self.task_id += 1;
        self.tasks.push((
            Process::new(self.task_id, State::Runnable),
            ProcessState::new(Box::new([0; 4096]), Box::new([0; 4096]), kernel_owned, target_fn) // FIXME: Make the kernel parameter configurable
        ));
        self.task_id
    }
}

#[repr(C)]
pub struct ProcessState {
    kernel_rsp: u64,
    kernel_top_rsp: u64,
    kernel_stack: Box<[u8]>,
    user_stack: Box<[u8]>,
}

impl ProcessState {
    fn new(mut kernel_stack: Box<[u8]>, mut user_stack: Box<[u8]>, kernel: bool, start_fn: fn()) -> Self {
        let kernel_addr = kernel_stack.as_mut().as_mut_ptr().expose_addr() + kernel_stack.len();
        {
            // FIXME: What about the direction flag?
            // TODO: Maybe change this (for io privilege level) when we work on io in userspace
            const DEFAULT_FLAGS: usize = 0 |
                (1 << 1) | // reserved
                (1 << 9);  // interrupt enable flag
            // in hex: 0x0202

            let kernel_stack: *mut usize = ptr::from_exposed_addr_mut(kernel_addr);

            let mut code_selector = if kernel {
                KERNEL_CODE_SEGMENT_IDX * 8
            } else {
                USER_CODE_SEGMENT_IDX * 8
            };
            code_selector |= if kernel {
                0
            } else {
                3
            };

            unsafe {
                // https://www.felixcloutier.com/x86/iret:iretd
                // https://wiki.osdev.org/Interrupt_Service_Routines
                // setup the stack frame iret expects
                kernel_stack.offset(-0).write(
                    if kernel {
                        // FIXME: Is this the correct thing to do if the privilege level doesn't change?
                        kernel_addr as usize
                    } else {
                        user_stack.as_mut().as_mut_ptr().expose_addr() as usize
                    });                   // rsp (for user stack)
                kernel_stack.offset(-1).write(DEFAULT_FLAGS);
                kernel_stack.offset(-2).write(code_selector);
                kernel_stack.offset(-3).write(
                    (start_fn as *const ()).expose_addr() as usize);       // rip

                const INTERRUPT_FRAME_OFFSET: isize = 4;

                // setup registers
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 0).write(0);                                // rax
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 1).write(0);                                // rbx
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 2).write(0);                                // rcx
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 3).write(0);                                // rdx
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 4).write(0);                                // rsi
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 5).write(0);                                // rdi
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 6).write(0);                                // r8
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 7).write(0);                                // r9
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 8).write(0);                                // r10
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 9).write(0);                                // r11
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 10).write(0);                               // r12
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 11).write(0);                               // r13
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 12).write(0);                               // r14
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 13).write(0);                               // r15

                if !kernel && false {
                    let address_space = mem::setup_user_address_space().start_address.as_u64() as usize;
                    kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - 14).write(address_space);               // cr3
                }
                let final_offset = if kernel || true {
                    14
                } else {
                    15
                };
                kernel_stack.offset(-INTERRUPT_FRAME_OFFSET - final_offset).write(kernel_addr);            // rbp

                // FIXME: (THIS IS JUST A NOTE) IMPORTANT: RBP IS *NOTHING* SPECIAL its just a general purpose register
            }
        }
        const INTERRUPT_FRAME_OFFSET: isize = 4;

        let final_offset = if kernel || true {
            14
        } else {
            15
        };

        Self {
            kernel_rsp: (kernel_addr - size_of::<usize>() * (final_offset + INTERRUPT_FRAME_OFFSET) as usize) as u64,
            kernel_top_rsp: (kernel_addr + kernel_stack.len()) as u64,
            kernel_stack,
            user_stack,
        }
    }
}

struct SchedulerEntry {
    process: Process,
    state: ProcessState,
    balance: u64,
}

/// This function is for testing purposes only!
pub fn start_proc(target: fn(), kernel_owned: bool) {
    SCHEDULER
        .lock()
        .start_process(target, kernel_owned);
}

fn idle() {
    loop {
        // println!("idling...!");
        unsafe { wait_for_interrupt(); }
    }
}

fn get_idle_task() -> Arc<Mutex<(Process, ProcessState)>> {
    IDLE_TASK.call_once(|| {
        Arc::new(Mutex::new((Process::new(0, State::Runnable),
                             ProcessState::new(Box::new([0; 4096]), Box::new([0; 4096]), true, idle))))
    }).clone()
}

// FIXME: Make task per-core
static mut TASK: Option<(Process, ProcessState)> = None;

pub fn init() {
    unsafe { VOID_TASK.write(ProcessState::new(Box::new([0; 256]), Box::new([0; 0]), true, idle)); }; // FIXME: Use as little data as possible
}

fn get_scheduler() -> Arc<Mutex<Box<dyn Scheduler + Send>>> {
    SCHEDULER.clone()
}

#[no_mangle]
extern "C" fn select_next_task() -> *mut ProcessState {
    let next = get_scheduler().lock()
        .pick_next();

    let next = next.map_or_else(|| {
        replace_curr_task(None);
        addr_of_mut!(get_idle_task().clone().lock().1) // FIXME: This is a dirty workaround and potentially dangerous, improve this!
    }, |task| {
        replace_curr_task(Some(task));
        addr_of_mut!(unsafe { TASK.as_mut().unwrap() }.1)
    }) as *mut ProcessState;
    next
}

fn replace_curr_task(task: Option<(Process, ProcessState)>) {
    if let Some(old_task) = unsafe { TASK.take() } {
        get_scheduler().lock().reinsert_task(old_task);
    }
    unsafe { TASK = task; }
}

#[no_mangle]
pub extern "C" fn current_task_ptr() -> *mut ProcessState {
    if unlikely(unsafe { TASK.is_none() }) {
        if unlikely(!INIT.load(Ordering::Acquire)) {
            // we return an address to a void in order to prevent the current stack's address from being written to the first task's stack address
            INIT.store(true, Ordering::Release);
            // SAFETY:
            // it is safe to call as_mut_ptr here because current_task_ptr may only be called after
            // init was called which initializes VOID_TASK
            return unsafe { VOID_TASK.as_mut_ptr() };
        }
        let tmp = get_idle_task().clone();
        let mut tmp = tmp.lock();
        let tmp = addr_of_mut!(tmp.1);
        tmp
    } else {
        unsafe { addr_of_mut!(TASK.as_mut().unwrap().1) }
    }
}
