use crate::fs::{make_pipe, open_file, OpenFlags, Stat, StatMode, OSInode, ROOT_INODE};
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process, current_task, current_user_token};
use alloc::sync::Arc;
/// write syscall
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_write",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
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
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len, 0))) as isize
    } else {
        -1
    }
}
/// read syscall
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_read",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
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
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len, 1))) as isize
    } else {
        -1
    }
}
/// open sys
pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!(
        "kernel:pid[{}] sys_open",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}
/// close syscall
pub fn sys_close(fd: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_close",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}
/// pipe syscall
pub fn sys_pipe(pipe: *mut usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_pipe",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(pipe_read);
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(pipe_write);
    *translated_refmut(token, pipe) = read_fd;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    0
}
/// dup syscall
pub fn sys_dup(fd: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_dup",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    let new_fd = inner.alloc_fd();
    inner.fd_table[new_fd] = Some(Arc::clone(inner.fd_table[fd].as_ref().unwrap()));
    new_fd as isize
}

// YOUR JOB: Get status of an open file.
pub fn sys_fstat(fd: usize, stat_buf: *mut Stat) -> isize {
    trace!("kernel:pid[{}] sys_fstat", current_task().unwrap().process.upgrade().unwrap().getpid());
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    let ino;
    let nlink;
    if let Some(file) = &inner.fd_table[fd] {
        let any: &dyn core::any::Any = file.as_any();
        let os_node = any.downcast_ref::<OSInode>().unwrap();
        ino = os_node.get_inode_id();
        let (block_id, block_offset) = os_node.get_inode_pos();
        nlink = ROOT_INODE.get_link_num(block_id, block_offset);
    } else {
        return -1;
    }
    let stat = &Stat {
        dev: 0,
        ino: ino,
        mode: StatMode::FILE,
        nlink: nlink,
        pad: [0;7],
    };
    let st = translated_byte_buffer(token, stat_buf as *const u8, core::mem::size_of::<Stat>(), 0);
    let stat_ptr = stat as *const _ as *const u8;
    for (idx, byte) in st.into_iter().enumerate() {
        unsafe {
            byte.copy_from_slice(core::slice::from_raw_parts(stat_ptr.wrapping_byte_add(idx), byte.len()));
        }
    }
    0
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(old_path: *const u8, new_path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_linkat", current_task().unwrap().process.upgrade().unwrap().getpid());
    let token = current_user_token();
    let old_path = translated_str(token, old_path);
    let new_path = translated_str(token, new_path);
    if old_path.as_str() == new_path.as_str() {
        return -1;
    }
    match open_file(old_path.as_str(), OpenFlags::RDONLY) {
        Some(_inode) => {
            let (new_path_dir, new_filename) = new_path.as_str().rsplit_once('/').unwrap_or(("", new_path.as_str()));
            if new_path_dir == "" {
                ROOT_INODE.link(old_path.as_str(), new_filename);
            }
            else {
                let new_path_dir_inode = match open_file(new_path_dir, OpenFlags::RDONLY) {
                    Some(inode) => inode,
                    None => return -1,
                };
                let dir_inode = new_path_dir_inode.get_inode();
                dir_inode.link(old_path.as_str(), new_filename);
            }
            0
        }
        None => -1,
    }
}

// YOUR JOB: Remove a link.
pub fn sys_unlinkat(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_unlinkat", current_task().unwrap().process.upgrade().unwrap().getpid());
    let token = current_user_token();
    let path_str = translated_str(token, path);
    let (dir, filename) = path_str.as_str().rsplit_once('/').unwrap_or(("", path_str.as_str()));
    if dir == "" {
        ROOT_INODE.unlink(filename);
        return 0;
    }
    match open_file(dir, OpenFlags::RDONLY) {
        Some(inode) => {
            let dir_inode = inode.get_inode();
            dir_inode.unlink(filename);
            0
        }
        None => -1,
    }
}
