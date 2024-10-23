//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the whole operating system.
//!
//! A single global instance of [`Processor`] called `PROCESSOR` monitors running
//! task(s) for each core.
//!
//! A single global instance of `PID_ALLOCATOR` allocates pid for user apps.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.
mod context;
mod id;
mod manager;
mod processor;
mod switch;
#[allow(clippy::module_inception)]
#[allow(rustdoc::private_intra_doc_links)]
mod task;

use crate::{
    loader::get_app_data_by_name,
    mm::{MapPermission, VirtAddr, VirtPageNum},
    timer::get_time_ms,
};
use crate::fs::{open_file, OpenFlags};
use alloc::sync::Arc;
pub use context::TaskContext;
use lazy_static::*;
pub use manager::{fetch_task, TaskManager};
use switch::__switch;
use task::TaskInfo;
pub use task::{TaskControlBlock, TaskStatus};

pub use id::{kstack_alloc, pid_alloc, KernelStack, PidHandle};
pub use manager::add_task;
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
    Processor,
};
/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    // There must be an application running.
    let task = take_current_task().unwrap();

    // ---- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    // Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- release current PCB

    // push back to ready queue.
    add_task(task);
    // jump to scheduling cycle
    schedule(task_cx_ptr);
}

/// get current task info
pub fn get_current_task_info() -> Option<TaskInfo> {
    if let Some(current_task) = current_task() {
        Some(current_task.inner_exclusive_access().task_info.clone())
    } else {
        None
    }
}

/// pid of usertests app in make run TEST=1
pub const IDLE_PID: usize = 0;

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next(exit_code: i32) {
    // take from Processor
    let task = take_current_task().unwrap();

    let pid = task.getpid();
    if pid == IDLE_PID {
        println!(
            "[kernel] Idle process exit with exit_code {} ...",
            exit_code
        );
        panic!("All applications completed!");
    }

    // **** access current TCB exclusively
    let mut inner = task.inner_exclusive_access();
    // Change status to Zombie
    inner.task_status = TaskStatus::Zombie;
    // Record exit code
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    // ++++++ access initproc TCB exclusively
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    // ++++++ release parent PCB

    inner.children.clear();
    // deallocate user space
    inner.memory_set.recycle_data_pages();
    // drop file descriptors
    inner.fd_table.clear();
    drop(inner);
    // **** release current PCB
    // drop task manually to maintain rc correctly
    drop(task);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    /// Creation of initial process
    ///
    /// the name "initproc" may be changed to any other app name like "usertests",
    /// but we have user_shell, so we don't need to change it.
    pub static ref INITPROC: Arc<TaskControlBlock> = Arc::new({
        let inode = open_file("ch6b_initproc", OpenFlags::RDONLY).unwrap();
        let v = inode.read_all();
        TaskControlBlock::new(v.as_slice())
    });
}

///Add init process to the manager
pub fn add_initproc() {
    add_task(INITPROC.clone());
}

/// get current task status
pub fn get_current_task_status() -> Option<TaskStatus> {
    let current_task = current_task();
    if let Some(task) = current_task {
        let inner = task.inner_exclusive_access();
        Some(inner.task_status.clone())
    } else {
        None
    }
}

/// get current task ctx
pub fn get_current_task_ctx() -> Option<TaskContext> {
    let current = current_task();
    if let Some(task) = current {
        Some(task.inner_exclusive_access().task_cx.clone())
    } else {
        None
    }
}

/// get current task TCB
// pub fn get_current_task_tcb() -> Option<TaskControlBlock> {
//     let inner = TASK_MANAGER.inner.exclusive_access();
//     let ret_val = inner.tasks.get(inner.current_task).cloned();
//     drop(inner);
//     ret_val
// }

/// get current task id
pub fn get_current_task_id() -> usize {
    let pid = current_task().unwrap().pid.0;
    pid
}

/// set syscall id and time when syscall occur
pub fn set_current_task_info(syscall_id: usize) {
    let current_task = current_task();
    if let Some(tcb) = current_task {
        let mut task_inner = tcb.inner_exclusive_access();
        if task_inner.start_time == 0 {
            task_inner.start_time = get_time_ms();
        }
        task_inner.end_time = get_time_ms();
        let start_time = task_inner.start_time;
        let end_time = task_inner.end_time;
        let time = end_time - start_time;
        let task_status = task_inner.task_status.clone();
        let task_info = &mut task_inner.task_info;
        task_info.syscall_times[syscall_id] += 1;
        task_info.time = time;
        task_info.status = task_status;
    }
}

/// insert freamd page area from virtaddr range
pub fn insert_framed_area(start_va: VirtAddr, end_va: VirtAddr, prot: usize) -> isize {
    if let Some(tcb) = current_task() {
        let memory_set = &mut tcb.inner_exclusive_access().memory_set;
        let prot: u8 = prot as u8 | (MapPermission::U | MapPermission::V).bits();
        let permission = MapPermission::from_bits(prot).unwrap();
        memory_set.insert_framed_area(start_va, end_va, permission)
    } else {
        -1
    }
}

/// unmap a page
pub fn un_map(vpn: VirtPageNum) -> isize {
    if let Some(current_task) = current_task() {
        let memory_set = &mut current_task.inner_exclusive_access().memory_set;
        let page_table = &mut memory_set.page_table;
        if let Some(area) = memory_set
            .areas
            .iter_mut()
            .find(|area| area.vpn_range.get_start() == vpn)
        {
            area.unmap_one(page_table, vpn);
            return 0;
        } else {
            println!("not found maparea {:?}", vpn);
            return -1;
        }
    } else {
        -1
    }
}
