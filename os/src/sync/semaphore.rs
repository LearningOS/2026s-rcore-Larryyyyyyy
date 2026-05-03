//! Semaphore

use crate::sync::UPSafeCell;
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::{VecDeque, BTreeMap}, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
    pub allocations: BTreeMap<usize, usize>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    wait_queue: VecDeque::new(),
                    allocations: BTreeMap::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self, res_id: usize, deadlock_detect: bool) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();
        inner.count += 1;
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        if deadlock_detect {
            if let Some(allocation) = inner.allocations.get_mut(&tid) {
                *allocation -= 1;
                if *allocation == 0 {
                    inner.allocations.remove(&tid);
                }
            } else {
                panic!("current task does not hold the semaphore!");
            }
        }
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                if deadlock_detect {
                    let target_tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
                    let allocation = inner.allocations.entry(target_tid).or_insert(0);
                    *allocation += 1;
                    let process = task.process.upgrade().unwrap();
                    let mut process_inner = process.inner_exclusive_access();
                    process_inner.need[target_tid][res_id] -= 1;
                }
                wakeup_task(task);
            }
        }
    }

    /// down operation of semaphore
    pub fn down(&self, res_id: usize, deadlock_detect: bool) {
        trace!("kernel: Semaphore::down");
        let mut inner = self.inner.exclusive_access();
        inner.count -= 1;
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        } else {
            if deadlock_detect {
                let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
                let allocation = inner.allocations.entry(tid).or_insert(0);
                *allocation += 1;
                let process = current_task().unwrap().process.upgrade().unwrap();
                let mut process_inner = process.inner_exclusive_access();
                process_inner.need[tid][res_id] -= 1;
            }
        }
    }

    /// look out the count of the semaphore
    pub fn get_count(&self) -> isize {
        self.inner.exclusive_access().count
    }
}

