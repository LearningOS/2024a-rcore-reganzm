//! File and filesystem-related syscalls
use crate::fs::{open_file, OSInode, OpenFlags, Stat, StatMode};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::syscall::get_pa_from_va;
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    println!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    let mut result: isize = -1;
    unsafe {
        let stat = get_pa_from_va(st as usize) as *mut Stat;
        if let Some(task) = current_task() {
            let fd_table = &task.inner_exclusive_access().fd_table;
            if let Some(fd) = fd_table[fd].clone() {
                if let Some(osinode) = fd.as_any().downcast_ref::<OSInode>() {
                    let osinner = osinode.inner.exclusive_access();
                    let inode_id = osinner.inode.inode_id;
                    // ino
                    (*stat).ino = inode_id as u64;
                    // mode
                    let disk_inode_type = osinner.inode.get_disk_inode_mode();
                    let file_mode = match disk_inode_type {
                        // -1 File
                        // 1 Dir
                        -1 => StatMode::FILE,
                        1 => StatMode::DIR,
                        _ => StatMode::NULL,
                    };
                    (*stat).mode = file_mode;
                    // nlink
                    let root_inode_id = osinner.inode.get_root_inode();
                    let nlinks = root_inode_id.hard_link_count(inode_id);
                    println!("nlinks : {} inode_id :{} ", nlinks, inode_id);
                    (*stat).nlink = nlinks as u32;
                }
            }
            // dev
            (*stat).dev = 0;

            // nlink
            println!("stat:{:?}", *stat);

            result = 0;
        }
    }
    result
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(old_name: *const u8, new_name: *const u8) -> isize {
    let mut result = -1;
    let token = current_user_token();
    let s_old_name = translated_str(token, old_name);
    let s_new_name = translated_str(token, new_name);
    if let Some(osinode) = open_file(&s_old_name, OpenFlags::RDWR) {
        let root_inode = osinode.inner.exclusive_access().inode.get_root_inode();
        result = root_inode.hard_link(s_old_name.as_str(), s_new_name.as_str());
    }
    result
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(name: *const u8) -> isize {
    let mut result = -1;
    let token = current_user_token();
    let s_name = translated_str(token, name);
    if let Some(osinode) = open_file(&s_name, OpenFlags::RDWR) {
        let root_inode = osinode.inner.exclusive_access().inode.get_root_inode();
        result = root_inode.hard_un_link(&s_name);
    }
    result
}
