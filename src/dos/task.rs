//! Task management and round-robin scheduler for Setun-DOS.
//!
//! Replicates KolibriOS `sched.inc` (`find_next_task`, `do_change_task`)
//! for the Setun 21-trit register architecture.

use crate::cpu::{Cpu, INDEX_REGISTER_COUNT};
use crate::word::Word21;

/// Execution status of a Setun-DOS task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    /// Actively executing instructions.
    Running,
    /// Ready to run in scheduler queue.
    Ready,
    /// Waiting for I/O or timer event.
    Blocked,
    /// Terminated with exit code.
    Exited(i64),
}

/// Process Control Block (PCB) recording the complete hardware context of a Setun task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessControlBlock {
    pub pid: usize,
    pub name: String,
    pub state: TaskState,
    pub saved_pc: i64,
    pub saved_sp: i64,
    pub saved_acc: Word21,
    pub saved_r: Word21,
    pub saved_ix: [Word21; INDEX_REGISTER_COUNT],
    pub heap_brk: i64,
    pub total_cycles: u64,
}

impl ProcessControlBlock {
    /// Creates a new task ready for scheduling.
    pub fn new(pid: usize, name: String, entry_pc: i64, sp: i64, heap_brk: i64) -> Self {
        Self {
            pid,
            name,
            state: TaskState::Ready,
            saved_pc: entry_pc,
            saved_sp: sp,
            saved_acc: Word21::ZERO,
            saved_r: Word21::ZERO,
            saved_ix: [Word21::ZERO; INDEX_REGISTER_COUNT],
            heap_brk,
            total_cycles: 0,
        }
    }

    /// Saves the current CPU registers into the PCB.
    pub fn save_context(&mut self, cpu: &Cpu, heap_brk: i64) {
        self.saved_pc = cpu.pc;
        self.saved_sp = cpu.sp;
        self.saved_acc = cpu.acc;
        self.saved_r = cpu.r;
        self.saved_ix = cpu.index_regs;
        self.heap_brk = heap_brk;
        self.total_cycles += cpu.cycles;
    }

    /// Restores CPU registers from this PCB.
    pub fn restore_context(&self, cpu: &mut Cpu) {
        cpu.pc = self.saved_pc;
        cpu.sp = self.saved_sp;
        cpu.acc = self.saved_acc;
        cpu.r = self.saved_r;
        cpu.index_regs = self.saved_ix;
        cpu.halted = false;
        cpu.trap = None;
    }
}

/// Multi-task scheduler managing tasks in round-robin sequence.
pub struct TaskScheduler {
    pub tasks: Vec<ProcessControlBlock>,
    pub current_idx: usize,
    next_pid: usize,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current_idx: 0,
            next_pid: 1,
        }
    }

    /// Spawns a new task in the scheduler.
    pub fn spawn(&mut self, name: &str, entry_pc: i64, sp: i64, heap_brk: i64) -> usize {
        let pid = self.next_pid;
        self.next_pid += 1;
        let pcb = ProcessControlBlock::new(pid, name.to_string(), entry_pc, sp, heap_brk);
        self.tasks.push(pcb);
        pid
    }

    /// Returns a reference to the currently active task.
    pub fn current(&self) -> Option<&ProcessControlBlock> {
        self.tasks.get(self.current_idx)
    }

    /// Returns a mutable reference to the currently active task.
    pub fn current_mut(&mut self) -> Option<&mut ProcessControlBlock> {
        self.tasks.get_mut(self.current_idx)
    }

    /// Saves current CPU context into active task and selects the next ready task.
    pub fn switch_next(&mut self, cpu: &mut Cpu, current_heap_brk: &mut i64) -> bool {
        if self.tasks.is_empty() {
            return false;
        }

        // Save current task context
        if let Some(curr) = self.tasks.get_mut(self.current_idx) {
            if curr.state == TaskState::Running {
                curr.state = TaskState::Ready;
            }
            curr.save_context(cpu, *current_heap_brk);
        }

        // Find next Ready task in round-robin fashion
        let total = self.tasks.len();
        for offset in 1..=total {
            let candidate_idx = (self.current_idx + offset) % total;
            if self.tasks[candidate_idx].state == TaskState::Ready {
                self.current_idx = candidate_idx;
                let next_task = &mut self.tasks[candidate_idx];
                next_task.state = TaskState::Running;
                next_task.restore_context(cpu);
                *current_heap_brk = next_task.heap_brk;
                return true;
            }
        }

        // If no other task is ready, remain on current if still runnable
        if let Some(curr) = self.tasks.get_mut(self.current_idx) {
            if curr.state == TaskState::Ready {
                curr.state = TaskState::Running;
                curr.restore_context(cpu);
                *current_heap_brk = curr.heap_brk;
                return true;
            }
        }

        false
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}
