use crossbeam_queue::SegQueue;

use crate::lowlevel::Stack;

static QUEUE: SegQueue<Stack> = SegQueue::new();

pub fn get() -> crate::lowlevel::Stack
{
    QUEUE.pop().unwrap_or(Stack::new())
}

pub fn put(stack: crate::lowlevel::Stack)
{
    QUEUE.push(stack);
}
