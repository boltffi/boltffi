use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

#[repr(align(64))]
struct CacheLinePadded<T>(T);

impl<T> CacheLinePadded<T> {
    fn new(value: T) -> Self {
        Self(value)
    }

    fn get(&self) -> &T {
        &self.0
    }
}

pub struct SpscRingBuffer<T> {
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    capacity: usize,
    capacity_mask: usize,
    producer_index: CacheLinePadded<AtomicUsize>,
    consumer_index: CacheLinePadded<AtomicUsize>,
}

unsafe impl<T: Send> Send for SpscRingBuffer<T> {}
unsafe impl<T: Send> Sync for SpscRingBuffer<T> {}

impl<T> SpscRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        let buffer = (0..capacity).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect::<Vec<_>>().into_boxed_slice();

        Self {
            buffer,
            capacity,
            capacity_mask: capacity - 1,
            producer_index: CacheLinePadded::new(AtomicUsize::new(0)),
            consumer_index: CacheLinePadded::new(AtomicUsize::new(0)),
        }
    }

    #[inline]
    fn slot_index(&self, sequence: usize) -> usize {
        sequence & self.capacity_mask
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let producer_position = self.producer_index.get().load(Ordering::Relaxed);
        let consumer_position = self.consumer_index.get().load(Ordering::Acquire);

        let available_slots = self.capacity - (producer_position - consumer_position);
        if available_slots == 0 {
            return Err(value);
        }

        let slot_index = self.slot_index(producer_position);
        unsafe {
            (*self.buffer[slot_index].get()).write(value);
        }

        self.producer_index.get().store(producer_position + 1, Ordering::Release);

        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let consumer_position = self.consumer_index.get().load(Ordering::Relaxed);
        let producer_position = self.producer_index.get().load(Ordering::Acquire);

        if consumer_position == producer_position {
            return None;
        }

        let slot_index = self.slot_index(consumer_position);
        let value = unsafe { (*self.buffer[slot_index].get()).assume_init_read() };

        self.consumer_index.get().store(consumer_position + 1, Ordering::Release);

        Some(value)
    }

    pub fn pop_batch_into(&self, output_buffer: &mut [MaybeUninit<T>]) -> usize {
        let consumer_position = self.consumer_index.get().load(Ordering::Relaxed);
        let producer_position = self.producer_index.get().load(Ordering::Acquire);

        let available_count = producer_position - consumer_position;
        let batch_size = available_count.min(output_buffer.len());

        if batch_size == 0 {
            return 0;
        }

        output_buffer.iter_mut().take(batch_size).enumerate().for_each(|(offset, output_slot)| {
            let slot_index = self.slot_index(consumer_position + offset);
            let value = unsafe { (*self.buffer[slot_index].get()).assume_init_read() };
            output_slot.write(value);
        });

        self.consumer_index.get().store(consumer_position + batch_size, Ordering::Release);

        batch_size
    }

    pub fn available_count(&self) -> usize {
        let producer_position = self.producer_index.get().load(Ordering::Acquire);
        let consumer_position = self.consumer_index.get().load(Ordering::Acquire);
        producer_position - consumer_position
    }

    pub fn is_empty(&self) -> bool {
        self.available_count() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T> Drop for SpscRingBuffer<T> {
    fn drop(&mut self) {
        let consumer_position = self.consumer_index.get().load(Ordering::Relaxed);
        let producer_position = self.producer_index.get().load(Ordering::Relaxed);

        (consumer_position..producer_position).for_each(|position| {
            let slot_index = self.slot_index(position);
            unsafe {
                (*self.buffer[slot_index].get()).assume_init_drop();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop_single() {
        let ring_buffer = SpscRingBuffer::<i32>::new(4);
        assert!(ring_buffer.push(42).is_ok());
        assert_eq!(ring_buffer.pop(), Some(42));
        assert_eq!(ring_buffer.pop(), None);
    }

    #[test]
    fn test_push_until_full() {
        let ring_buffer = SpscRingBuffer::<i32>::new(4);
        assert!(ring_buffer.push(1).is_ok());
        assert!(ring_buffer.push(2).is_ok());
        assert!(ring_buffer.push(3).is_ok());
        assert!(ring_buffer.push(4).is_ok());
        assert!(ring_buffer.push(5).is_err());
    }

    #[test]
    fn test_fifo_order() {
        let ring_buffer = SpscRingBuffer::<i32>::new(8);
        (0..5).for_each(|index| {
            ring_buffer.push(index).unwrap();
        });
        (0..5).for_each(|expected| {
            assert_eq!(ring_buffer.pop(), Some(expected));
        });
    }

    #[test]
    fn test_pop_batch() {
        let ring_buffer = SpscRingBuffer::<i32>::new(16);
        (0..10).for_each(|index| {
            ring_buffer.push(index).unwrap();
        });

        let mut batch_buffer: [MaybeUninit<i32>; 4] = unsafe { MaybeUninit::uninit().assume_init() };
        let popped_count = ring_buffer.pop_batch_into(&mut batch_buffer);
        assert_eq!(popped_count, 4);

        batch_buffer.iter().take(popped_count).enumerate().for_each(|(index, slot)| {
            assert_eq!(unsafe { slot.assume_init() }, index as i32);
        });

        assert_eq!(ring_buffer.available_count(), 6);
    }

    #[test]
    fn test_wraparound() {
        let ring_buffer = SpscRingBuffer::<i32>::new(4);

        (0..3).for_each(|index| ring_buffer.push(index).unwrap());
        (0..3).for_each(|_| {
            ring_buffer.pop();
        });

        (10..14).for_each(|index| ring_buffer.push(index).unwrap());
        (10..14).for_each(|expected| {
            assert_eq!(ring_buffer.pop(), Some(expected));
        });
    }
}
