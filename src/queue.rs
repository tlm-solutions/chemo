use crate::{GrpcGpsPoint, R09GrpcTelegram};

use log::info;
use std::time::{SystemTime, UNIX_EPOCH};

///
/// trait that enforces the type to have a get_time function
/// that returns the time in milliseconds
///
pub trait GetTime {
    fn get_time(&self) -> u128 ;
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
    T: GetTime,
{
    pub fn new() -> TimeQueue<T> {
        const DEFAULT_TIME: u128 = 1000; // 1s
        TimeQueue {
            time_buffer: DEFAULT_TIME,
            elements: vec![],
        }
    }

    pub fn insert(&mut self, element: T) {
        self.elements.push(element);
        self.elements.sort_by(|a, b| a.get_time().cmp(&b.get_time()));
    }

    pub fn pop(&mut self) -> Option<T> {
        let get_time = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis()
        };

        if let Some(element) = self.elements.last() {
            info!("len: {}, oldest element: {}", self.elements.len(), get_time() - element.get_time());
            if (get_time() - element.get_time() as u128) < self.time_buffer {
                None
            } else {
                self.elements.pop()
            }
        } else {
            None
        }
    }
}
