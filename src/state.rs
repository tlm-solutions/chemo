use crate::queue::TimeQueue;

use tlms::grpc::receive_waypoint_client::ReceiveWaypointClient;
use tlms::grpc::{GrpcGpsPoint, GrpcWaypoint, R09GrpcTelegram};

use log::error;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

///
/// State that is saved per Vehicle
///
pub struct VehicleState {
    /// when this struct was last updated
    last_update: u64,
    /// when the last gps point was received
    last_gps_update: u64,
    /// delay of the vehicle the gps points get annotated with this data
    delay: Option<f32>,
}

///
/// Mapping from (line, run) -> VehicleState
///
pub type Vehicles = HashMap<(i32, i32), VehicleState>;

pub type QueueR09 = Arc<Mutex<TimeQueue<R09GrpcTelegram>>>;
pub type QueueGps = Arc<Mutex<TimeQueue<GrpcGpsPoint>>>;

pub struct State {
    /// queue for r09 telegrams
    r09_queue: QueueR09,
    /// queue for raw gps points
    gps_queue: QueueGps,
    /// mapping to keep information about the vehicles
    vehicles: Vehicles,
    /// list of hosts that want to receive waypoints
    grpc_sinks: Vec<String>,
}

const DISCARD_R09_TIME: u64 = 60;

impl State {
    /// creates empty state object
    pub fn new(r09_queue: QueueR09, gps_queue: QueueGps) -> State {
        let mut grpc_sinks = Vec::new();

        for (k, v) in std::env::vars() {
            if k.starts_with("GRPC_HOST_") {
                grpc_sinks.push(v);
            }
        }

        State {
            r09_queue,
            gps_queue,
            vehicles: HashMap::new(),
            grpc_sinks,
        }
    }

    pub async fn processing_loop(&mut self) {
        let get_time = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis()
        };

        loop {
            const MAX_QUEUE_PROCESSING_TIME_SLICE: u128 = 500;

            //TODO: maybe optimize this later to remove code redudency

            let start_time = get_time();

            while get_time() - start_time > MAX_QUEUE_PROCESSING_TIME_SLICE {
                let mut gps_point = None;
                if let Ok(mut queue) = self.gps_queue.try_lock() {
                    gps_point = queue.pop();
                }

                match gps_point {
                    Some(value) => {
                        self.handle_gps(value).await;
                    }
                    None => {
                        break;
                    }
                }
            }

            let start_time = get_time();
            while get_time() - start_time > MAX_QUEUE_PROCESSING_TIME_SLICE {
                let mut r09_telegram = None;
                if let Ok(mut queue) = self.r09_queue.try_lock() {
                    r09_telegram = queue.pop();
                }

                match r09_telegram {
                    Some(value) => {
                        self.handle_r09(value).await;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_r09(&mut self, telegram: R09GrpcTelegram) {
        // function that converts the r09 delay value into seconds
        let convert_delay = |enum_value| (enum_value - 7) as f32 * 60f32;

        match self
            .vehicles
            .get_mut(&(telegram.line(), telegram.run_number()))
        {
            Some(vehicle_information) => {
                // vehicle was seen before

                // if we can send r09_times
                let send_r09 =
                    (telegram.time - vehicle_information.last_gps_update) > DISCARD_R09_TIME;

                // updating the state
                vehicle_information.last_update = telegram.time;
                vehicle_information.delay = telegram.delay.map(convert_delay);

                if send_r09 {
                    self.send_waypoint(GrpcWaypoint {
                        id: 0u64,
                        source: 0i32, // TODO USE ENUM HERE
                        time: telegram.time,
                        lat: 0.0f32,
                        lon: 0.0f32,
                        line: telegram.line(),
                        run: telegram.run_number(),
                        delayed: telegram.delay.map(convert_delay),
                    })
                    .await;
                }
            }
            None => {
                // vehicle was never seen before

                let vehicle_information = VehicleState {
                    last_update: telegram.time,
                    last_gps_update: 0,
                    delay: telegram.delay.map(convert_delay),
                };

                self.vehicles.insert(
                    (telegram.line(), telegram.run_number()),
                    vehicle_information,
                );
            }
        }
    }

    async fn handle_gps(&mut self, point: GrpcGpsPoint) {
        let mut delay = None;

        match self.vehicles.get_mut(&(point.line, point.run)) {
            Some(vehicle_information) => {
                vehicle_information.last_gps_update = point.time;
                vehicle_information.last_update = point.time;

                delay = vehicle_information.delay;
            }
            None => {
                let vehicle_information = VehicleState {
                    last_update: point.time,
                    last_gps_update: point.time,
                    delay: None,
                };

                self.vehicles
                    .insert((point.line, point.run), vehicle_information);
            }
        }

        self.send_waypoint(GrpcWaypoint {
            id: 0u64,
            source: 0i32, // TODO USE ENUM HERE
            time: point.time,
            lat: point.lat,
            lon: point.lon,
            line: point.line,
            run: point.run,
            delayed: delay,
        })
        .await;
    }

    async fn send_waypoint(&self, waypoint: GrpcWaypoint) {
        for url in &self.grpc_sinks {
            match ReceiveWaypointClient::connect(url.clone()).await {
                Ok(mut client) => {
                    let request = tonic::Request::new(waypoint.clone());

                    if let Err(e) = client.receive_waypoint(request).await {
                        error!("error while sending data to {:?} with error {:?}", &url, e);
                    }
                }
                Err(e) => {
                    error!("cannot connect to waypoint sink with error {:?}", e);
                }
            }
        }
    }
}
