use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Resource, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};
use alloc::sync::Arc;
/// sleep syscall
pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    0
}
/// mutex create syscall
pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new()))
    } else {
        Some(Arc::new(MutexBlocking::new()))
    };
    let mut process_inner = process.inner_exclusive_access();
    let mutex_id = if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    };

    process_inner
        .deadlock_detector
        .add_resource(Resource::Mutex(mutex_id as usize), 1);
    process_inner.deadlock_detector.display(get_tid());

    mutex_id
}

fn get_tid() -> usize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid
}

/// mutex lock syscall
pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let mut result: isize = -1;
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let detected_deadlock_flag = process_inner.deadlock_detection_enabled;
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let task_id = get_tid();
    // detect mutex deadlock
    if detected_deadlock_flag {
        let try_result =
            process_inner
                .deadlock_detector
                .try_request(task_id, 1, Resource::Mutex(mutex_id));
        process_inner.deadlock_detector.display(task_id);
        println!("try_result------>{}", try_result);
        if try_result {
            result = 0;
            process_inner
                .deadlock_detector
                .request(task_id, 1, Resource::Mutex(mutex_id));
            process_inner.deadlock_detector.display(task_id);
            mutex.lock();
        } else {
            result = -0xDEAD;
        }
    } else {
        mutex.lock();
    }
    process_inner.deadlock_detector.display(task_id);
    drop(process_inner);
    drop(process);

    result
}
/// mutex unlock syscall
pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    let task_id = get_tid();
    if process_inner.deadlock_detection_enabled {
        process_inner
            .deadlock_detector
            .release_resource(task_id, 1, Resource::Mutex(mutex_id));
    }
    process_inner.deadlock_detector.display(task_id);
    mutex.unlock();
    0
}
/// semaphore create syscall
pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };
    process_inner
        .deadlock_detector
        .add_resource(Resource::Semaphore(id), res_count);
    process_inner.deadlock_detector.display(get_tid());
    id as isize
}
/// semaphore up syscall
pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let task_id = get_tid();
    if process_inner.deadlock_detection_enabled {
        process_inner
            .deadlock_detector
            .release_resource(task_id, 1, Resource::Semaphore(sem_id));
    }
    sem.up();
    process_inner.deadlock_detector.display(task_id);
    0
}
/// semaphore down syscall
pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let detected_dead_lock = process_inner.deadlock_detection_enabled;
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    let task_id = get_tid();
    // detected semaphore deadlock
    if detected_dead_lock {
        let try_result =
            process_inner
                .deadlock_detector
                .try_request(task_id, 1, Resource::Semaphore(sem_id));
        println!("try_result------>{}", try_result);
        process_inner.deadlock_detector.display(task_id);
        if try_result {
            process_inner
                .deadlock_detector
                .request(task_id, 1, Resource::Semaphore(sem_id));
            sem.down();
            process_inner.deadlock_detector.display(task_id);
            return 0;
        } else {
            return -0xDEAD;
        }
    } else {
        sem.down();
        return 0;
    }
}
/// condvar create syscall
pub fn sys_condvar_create() -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    id as isize
}
/// condvar signal syscall
pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    condvar.signal();
    0
}
/// condvar wait syscall
pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    condvar.wait(mutex);
    0
}
/// enable deadlock detection syscall
///
/// YOUR JOB: Implement deadlock detection, but might not all in this syscall
pub fn sys_enable_deadlock_detect(enabled: usize) -> isize {
    let mut result = -1;
    if enabled == 1 {
        // set detection deadlock flag = true
        current_process()
            .inner_exclusive_access()
            .deadlock_detection_enabled = true;
        result = 0
    }
    result
}
