//! Process management syscalls

use crate::config::{MAX_SYSCALL_NUM};
use crate::task::{exit_current_and_run_next, current_task, suspend_current_and_run_next, TaskStatus, current_user_token, mmap, munmap};
use crate::timer::get_time_us;
use crate::mm::{copy_kernel_to_user, VirtAddr};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

#[derive(Clone, Copy)]
pub struct TaskInfo {
    pub status: TaskStatus,
    pub syscall_times: [u32; MAX_SYSCALL_NUM],
    pub time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    info!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

// YOUR JOB: 引入虚地址后重写 sys_get_time
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    let us = get_time_us();
    let tmp = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    copy_kernel_to_user(current_user_token(), &tmp as *const TimeVal as *const u8, _ts as usize, core::mem::size_of::<TimeVal>());
    0
}

// CLUE: 从 ch4 开始不再对调度算法进行测试~
pub fn sys_set_priority(_prio: isize) -> isize {
    -1
}

// YOUR JOB: 扩展内核以实现 sys_mmap 和 sys_munmap
/* 
    申请内存
    参数：
    start 需要映射的虚存起始地址，要求按页对齐
    len 申请的字节长度
    port：第 0 位表示是否可读，第 1 位表示是否可写，第 2 位表示是否可执行。其他位无效且必须为 0
    返回值：执行成功则返回 0，错误返回 -1
*/
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    let start_va = VirtAddr::from(_start);
    if ! start_va.aligned() || _port & !0x7 != 0 || _port & 0x7 == 0 {
        return -1;
    }
    if _len == 0 {
        return 0;
    }

    let end_va = VirtAddr::from(_start+_len);
    mmap(start_va, end_va, _port)
}

pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    let start_va = VirtAddr::from(_start);
    if ! start_va.aligned() {
        return -1;
    }
    if _len == 0 {
        return 0;
    }
    let end_va = VirtAddr::from(usize::from(start_va)+_len);
    munmap(start_va, end_va)
}

// YOUR JOB: 引入虚地址后重写 sys_task_info
pub fn sys_task_info(ti: *mut TaskInfo) -> isize {
    let task = current_task();
    copy_kernel_to_user(current_user_token(), &task as *const TaskInfo as *const u8, ti as usize, core::mem::size_of::<TaskInfo>());
    0
}
