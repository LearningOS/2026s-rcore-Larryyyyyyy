//! Process management syscalls
use crate::task::{change_program_brk, 
                  exit_current_and_run_next, 
                  suspend_current_and_run_next, 
                  current_user_token, 
                  find_syscall_times, 
                  modify_current_task_mmap,
                  modify_current_task_munmap
                 };
use crate::timer::{get_time};
use crate::mm::{translated_byte_buffer, MapPermission, VirtAddr};
use crate::config::{PAGE_SIZE, CLOCK_FREQ};
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
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

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let ticks = get_time();
    let time_val = TimeVal {
        sec: ticks / CLOCK_FREQ,
        usec: (ticks % CLOCK_FREQ) * 1_000_000 / CLOCK_FREQ,
    };
    let buffers = translated_byte_buffer(current_user_token(), ts as *const u8, core::mem::size_of::<TimeVal>(), 1);
    let mut current_pos = 0;
    let src = &time_val as *const TimeVal as *const u8;
    for buffer in buffers {
        let len = buffer.len();
        unsafe {
            core::ptr::copy_nonoverlapping(
                src.add(current_pos),
                buffer.as_mut_ptr(),
                len
            );
        }
        current_pos += len;
    }
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            if id >= 0x0040_0000_0000 { return -1; }
            let buffers = translated_byte_buffer(current_user_token(), id as *const u8, 1, 0);
            if buffers.is_empty() { return -1; }
            buffers[0][0] as isize
        }
        1 => {
            if id >= 0x0040_0000_0000 { return -1; }
            let mut buffers = translated_byte_buffer(current_user_token(), id as *const u8, 1, 1);
            if buffers.is_empty() { return -1; }
            buffers[0][0] = (data & 0xFF) as u8;
            0
        }
        2 => {
            if id < 500 {
                find_syscall_times(id) as isize
            } else {
                -1
            }
        }
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap");
    if start % PAGE_SIZE != 0 {
        return -1
    }
    if (prot & !0x7 != 0) || (prot & 0x7 == 0) {
        return -1;
    }
    let mut permission = MapPermission::U;
    if prot & 0x1 != 0 {
        permission |= MapPermission::R;
    }
    if prot & 0x2 != 0 {
        permission |= MapPermission::W;
    }
    if prot & 0x4 != 0 {
        permission |= MapPermission::X;
    }
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    modify_current_task_mmap(start_va, end_va, permission)
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if start % PAGE_SIZE != 0 || len % PAGE_SIZE != 0 {
        return -1;
    }
    modify_current_task_munmap(start, len)
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

