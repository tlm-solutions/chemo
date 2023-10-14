use crate::{GrpcGpsPoint, R09GrpcTelegram};

use core::fmt::Debug;
use log::info;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

///
/// trait that enforces the type to have a get_time function
/// that returns the time in milliseconds
///
pub trait GetTime {
    fn get_time(&self) -> u128;
}

impl GetTime for R09GrpcTelegram {
    fn get_time(&self) -> u128 {
        self.time as u128
    }
}

impl GetTime for GrpcGpsPoint {
    fn get_time(&self) -> u128 {
        self.time as u128
    }
}

pub struct TimeQueue<T>
where
    T: GetTime,
{
    /// minimal duration time of an element inside the queue
    time_buffer: u128,
    /// list of elements that the queue currently containes
    elements: Vec<T>,
}

impl<T> TimeQueue<T>
where
    T: GetTime + Debug,
{
    pub fn new() -> TimeQueue<T> {
        const DEFAULT_TIME: u128 = 2000; // 2s
        TimeQueue {
            time_buffer: DEFAULT_TIME,
            elements: vec![],
        }
    }

    pub fn insert(&mut self, element: T) {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        
        // this ensures that elements inside the queue are not to old
        if element.get_time() > current_time - self.time_buffer {
            self.elements.push(element);
            self.elements.sort_by_key(|a| a.get_time())
        }
    }

    /// returns the top element
    pub fn pop(&mut self) -> Option<T> {
        let get_time = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis()
        };

        if let Some(element) = self.elements.first() {
            info!(
                "len: {}, oldest element: {}",
                self.elements.len(),
                get_time() - element.get_time()
            );
            if (get_time() - element.get_time()) < self.time_buffer {
                None
            } else {
                self.elements.pop()
            }
        } else {
            None
        }
    }

    /// returns the duration until the next event in the queue
    pub fn most_recent_event(&self) -> Duration {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();

        if let Some(element) = self.elements.last() {
            Duration::from_millis((time - element.get_time()) as u64)
        } else {
            Duration::from_millis(self.time_buffer as u64)
        }
    }

    /// this searches for element that satisifies this lambda
    /// mainly used to check if there are any gps points queued.
    pub fn find(&self, f: &dyn Fn(&T) -> bool) -> bool {
        self.elements.iter().any(f)
    }
}
