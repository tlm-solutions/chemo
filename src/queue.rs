use crate::{GrpcGpsPoint, R09GrpcTelegram};

use std::time::{SystemTime, UNIX_EPOCH};

///
/// trait that enforces the type to have a get_time function
/// that returns the time in milliseconds
///
pub trait GetTime {
    fn get_time(&self) -> u64;
}

impl GetTime for R09GrpcTelegram {
    fn get_time(&self) -> u64 {
        self.time
    }
}

impl GetTime for GrpcGpsPoint {
    fn get_time(&self) -> u64 {
        self.time
    }
}

pub struct TimeQueue<T>
where
    T: GetTime,
{
    /// minimal duration time of an element inside the queue
    time_buffer: u64,
    /// list of elements that the queue currently containes
    elements: Vec<T>,
}

impl<T> TimeQueue<T>
where
    T: GetTime,
{
    pub fn new() -> TimeQueue<T> {
        const DEFAULT_TIME: u64 = 1000; // 1s
        TimeQueue {
            time_buffer: DEFAULT_TIME,
            elements: vec![],
        }
    }

    pub fn insert(&mut self, element: T) {
        self.elements.push(element);
        self.elements.sort_by_key(|x| x.get_time());
    }

    pub fn pop(&mut self) -> Option<T> {
        let get_time = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis()
        };

        // TODO: remove this debug print
        /*println!(
            "{} {}",
            self.elements[0].get_time(),
            self.elements[self.elements.len() - 1].get_time()
        );*/

        if let Some(element) = self.elements.pop() {
            if (get_time() - element.get_time() as u128) < self.time_buffer.into() {
                None
            } else {
                Some(element)
            }
        } else {
            None
        }
    }
}
