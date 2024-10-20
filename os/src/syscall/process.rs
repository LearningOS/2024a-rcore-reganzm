//! Process management syscalls

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE, PAGE_SIZE_BITS},
    mm::{frame_alloc, PTEFlags, PageTable, PhysPageNum, VirtAddr},
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next, get_current_task_info, get_current_task_status, insert_framed_area, suspend_current_and_run_next, TaskStatus, TASK_MANAGER
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    pub status: TaskStatus,
    /// The numbers of syscall called by task
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    pub time: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn get_pa_from_va(va: usize) -> usize {
    let current_user_token = current_user_token();
    let current_page_table = PageTable::from_token(current_user_token);

    let vpn = VirtAddr::from(va).floor();
    let vpn_offset = VirtAddr::from(va).page_offset();

    let ppn = current_page_table.translate(vpn).unwrap().ppn().0;

    let pa = ppn << PAGE_SIZE_BITS | vpn_offset;
    pa
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    let ts_pa = get_pa_from_va(ts as usize) as *mut TimeVal;
    let us = get_time_us();
    unsafe {
        *ts_pa = TimeVal {
            sec: us / 1000000,
            usec: us % 1000000,
        };
    }
    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    unsafe {
        let ti_pa = get_pa_from_va(ti as usize) as *mut TaskInfo;
        // current task status
        let t_task_status = get_current_task_status();
        if t_task_status.is_none() {
            return -1;
        } else {
            (*ti_pa).status = t_task_status.unwrap();
        }
        // task syscalls times
        let t_task_info = get_current_task_info();
        if t_task_info.is_none() {
            return -1;
        } else {
            let info = t_task_info.unwrap();
            (*ti_pa).syscall_times = info.syscall_times;
            (*ti_pa).time = info.time;
            (*ti_pa).status = t_task_status.unwrap();
        }
    }
    0
}
// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, mut port: usize) -> isize {
    if len == 0 {
        return -1;
    }
    if port & 0x7 == 0 {
        return -1;
    }
    if port & !0x7 != 0 {
        return -1;
    }
    port = port << 1;
    // v r w x
    port &= 0xf;
    // U mode
    port |= 0x10;
    // avalable
    port |= 0x1;
    println!("port value ======> {:b}", port);
    insert_framed_area(VirtAddr::from(start), VirtAddr::from(start+len), port);
    println!("00000000000 return 0");
    return 0;
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
