//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    config::{MAX_SYSCALL_NUM, PAGE_SIZE_BITS},
    loader::get_app_data_by_name,
    mm::{translated_refmut, translated_str, PageTable, VirtAddr},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        get_current_task_info, get_current_task_status, insert_framed_area,
        suspend_current_and_run_next, un_map, TaskStatus,
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
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!(
        "kernel::pid[{}] sys_waitpid [{}]",
        current_task().unwrap().pid.0,
        pid
    );
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
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

/// use a virtual addr to get it's mapped physic addr
pub fn get_pa_from_va(va: usize) -> usize {
    let current_user_token = current_user_token();
    let current_page_table = PageTable::from_token(current_user_token);
    let vpn = VirtAddr::from(va).floor();
    let vpn_offset = VirtAddr::from(va).page_offset();
    let ppn = current_page_table.translate(vpn).unwrap().ppn().0;
    let pa = ppn << PAGE_SIZE_BITS | vpn_offset;
    pa
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

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, mut port: usize) -> isize {
    println!("start :{} len:{} port:{} ", start, len, port);
    if len == 0 {
        return -1;
    }
    // invald port
    if port & 0x7 == 0 {
        return -1;
    }
    // other bit must be 0
    if port & !0x7 != 0 {
        return -1;
    }
    // align with 4k
    if start & 0xfff != 0 {
        return -1;
    }
    // left shift 1bit the zero bit is valiable bit
    port = port << 1;
    // v r w x
    port &= 0xf;
    // U mode
    port |= 0x10;
    // avalable
    port |= 0x1;

    println!("port value ======> {:b}", port);
    let result = insert_framed_area(VirtAddr::from(start), VirtAddr::from(start + len), port);
    println!("00000000000 return {}", result);
    return result;
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    if start & 0xfff != 0 {
        return -1;
    }
    let start_vpn = VirtAddr::from(start).floor().0;
    let end_vpn = VirtAddr::from(start + len).ceil().0;
    let mut result = 0;
    for vpn in start_vpn..end_vpn {
        result = un_map(vpn.into());
    }
    result
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    -1
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    -1
}
