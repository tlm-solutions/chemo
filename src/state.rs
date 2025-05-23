use crate::queue::TimeQueue;

use tlms::grpc::receive_waypoint_client::ReceiveWaypointClient;
use tlms::grpc::{GrpcGpsPoint, GrpcWaypoint, R09GrpcTelegram};
use tlms::locations::{waypoint::WayPointType, TransmissionLocation};

use chrono::{Duration, Utc};
use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;
use diesel::{ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl};
use log::{debug, error};

use std::cmp::min;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// type of the postgres connection pool
type DbPool = r2d2::Pool<ConnectionManager<PgConnection>>;

/// construct the postgres connection pool
pub fn create_db_pool() -> DbPool {
    let default_postgres_host = String::from("localhost");
    let default_postgres_port = String::from("5432");
    let default_postgres_user = String::from("datacare");
    let default_postgres_database = String::from("tlms");
    let default_postgres_pw_path = String::from("/run/secrets/postgres_password");

    let password_path =
        env::var("CHEMO_POSTGRES_PASSWORD_PATH").unwrap_or(default_postgres_pw_path);
    let password = std::fs::read_to_string(password_path).expect("cannot read password file!");

    let database_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        env::var("CHEMO_POSTGRES_USER").unwrap_or(default_postgres_user),
        password,
        env::var("CHEMO_POSTGRES_HOST").unwrap_or(default_postgres_host),
        env::var("CHEMO_POSTGRES_PORT").unwrap_or(default_postgres_port),
        env::var("CHEMO_POSTGRES_DATABASE").unwrap_or(default_postgres_database)
    );

    debug!("Connecting to postgres database {}", &database_url);
    let manager = ConnectionManager::<PgConnection>::new(database_url);

    Pool::new(manager).expect("Failed to create pool.")
}

///
/// State that is saved per Vehicle
///
pub struct VehicleState {
    /// when this struct was last updated by a r09 telegram
    last_r09_update: u64,
    /// when the last gps point was received
    last_gps_update: u64,
    /// delay of the vehicle the gps points get annotated with this data
    delay: Option<f32>,
}

///
/// Mapping from region -> (line, run) -> VehicleState
///
pub type Vehicles = HashMap<i64, HashMap<(i32, i32), VehicleState>>;

pub type QueueR09 = Arc<Mutex<TimeQueue<R09GrpcTelegram>>>;
pub type QueueGps = Arc<Mutex<TimeQueue<GrpcGpsPoint>>>;

struct CachedLocation {
    location: (f64, f64),
    load_time: chrono::DateTime<Utc>,
}

pub struct State {
    /// queue for r09 telegrams
    r09_queue: QueueR09,
    /// queue for raw gps points
    gps_queue: QueueGps,
    /// mapping to keep information about the vehicles
    vehicles: Vehicles,
    /// list of hosts that want to receive waypoints
    grpc_sinks: Vec<String>,
    /// postgres connection pools
    db_pool: DbPool,
    /// local cache for r09 reporting point locations
    r09_reporting_points: HashMap<(i64, i32), CachedLocation>,
}

// TODO: maybe make those to configurable
// this constant defines the time difference that is needed to switch from gps stream to r09
const DISCARD_R09_TIME: u64 = 20 * 1000; // 20s

// after which time delays should not be valid anymore
const ACCEPT_DELAY: u64 = 90 * 1000; // 90s

impl State {
    /// creates empty state object
    pub fn new(r09_queue: QueueR09, gps_queue: QueueGps) -> State {
        let mut grpc_sinks = Vec::new();

        for (k, v) in std::env::vars() {
            if k.starts_with("CHEMO_GRPC_HOST_") {
                grpc_sinks.push(v);
            }
        }

        State {
            r09_queue,
            gps_queue,
            vehicles: HashMap::new(),
            grpc_sinks,
            db_pool: create_db_pool(),
            r09_reporting_points: HashMap::new(),
        }
    }

    /// main loop that fetches elements from the time-event loops
    /// and pushes them into the corresponding handlers.
    pub async fn processing_loop(&mut self) {
        let get_time = || {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis()
        };

        loop {
            const MAX_QUEUE_PROCESSING_TIME_SLICE: u128 = 50; //50ms

            // this in an optimization that chemo is not squatting a core entierally
            // here we fetch the nearest possible event in the queue and wait for its arrival
            let near_duration = min(
                self.gps_queue.lock().unwrap().most_recent_event(),
                self.r09_queue.lock().unwrap().most_recent_event(),
            );
            // waiting for the next event
            std::thread::sleep(near_duration);

            // processing gps points
            let start_time = get_time();
            while get_time() - start_time < MAX_QUEUE_PROCESSING_TIME_SLICE {
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

            // processing r09 telegrams in the queue
            let start_time = get_time();
            while get_time() - start_time < MAX_QUEUE_PROCESSING_TIME_SLICE {
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

    /// this makes a database look up to figure out the gps positions of a reporting_point
    async fn fetch_gps_position(
        &mut self,
        reporting_point: i32,
        region: i64,
    ) -> Option<(f64, f64)> {
        let mut database_connection = match self.db_pool.get() {
            Ok(conn) => conn,
            Err(e) => {
                error!("cannot get connection from connection pool {:?}", e);
                return None;
            }
        };

        const CACHING_TIME: i64 = 300;

        if let Some(value) = self.r09_reporting_points.get(&(region, reporting_point)) {
            if Utc::now() - value.load_time < Duration::seconds(CACHING_TIME) {
                return Some(value.location);
            }
        }

        use tlms::schema::r09_transmission_locations::dsl::r09_transmission_locations;
        use tlms::schema::r09_transmission_locations::{
            region as db_region, reporting_point as db_reporting_point,
        };

        match r09_transmission_locations
            .filter(db_region.eq(region))
            .filter(db_reporting_point.eq(reporting_point))
            .first::<TransmissionLocation>(&mut database_connection)
            .map(|row| (row.lat, row.lon))
        {
            Ok(location) => {
                self.r09_reporting_points.insert(
                    (region, reporting_point),
                    CachedLocation {
                        location,
                        load_time: Utc::now(),
                    },
                );
                Some(location)
            }
            Err(_) => None,
        }
    }

    /// handles r09 telegrams
    async fn handle_r09(&mut self, telegram: R09GrpcTelegram) {
        debug!("handling telegram {:?}", &telegram);

        // cannot work with this data discard instantly
        if telegram.line.is_none() || telegram.run_number.is_none() {
            return;
        }

        // function that converts the r09 delay value into seconds
        let convert_delay = |enum_value| enum_value as f32 * 60f32;

        // if this telegram should be send out as a waypoint
        let mut send_r09;

        match self
            .vehicles
            .entry(telegram.region)
            .or_insert_with(HashMap::new)
            .get_mut(&(telegram.line(), telegram.run_number()))
        {
            Some(vehicle_information) => {
                // vehicle was seen before

                // if we can send r09_times
                send_r09 = (telegram.time - vehicle_information.last_gps_update) > DISCARD_R09_TIME;

                // updating the state
                vehicle_information.last_r09_update = telegram.time;
                vehicle_information.delay = telegram.delay.map(convert_delay);
            }
            None => {
                // vehicle was never seen before

                let vehicle_information = VehicleState {
                    last_r09_update: telegram.time,
                    last_gps_update: 0,
                    delay: telegram.delay.map(convert_delay),
                };

                send_r09 = true;

                self.vehicles.get_mut(&telegram.region).unwrap().insert(
                    (telegram.line(), telegram.run_number()),
                    vehicle_information,
                );
            }
        }

        // check if a gps point is queued for the same vehicle
        if let Ok(gps_queue) = self.gps_queue.lock() {
            // so if we find a gps point for this vehicle we dont send the r09 telegram that why
            // its negated.
            send_r09 = !gps_queue.find(&|point: &GrpcGpsPoint| {
                point.line == telegram.line() || point.run == telegram.run_number()
            });
        }

        debug!("sending r09 telegram: {}", send_r09);

        // we send this r09 telegram as a waypoint
        if send_r09 {
            // getting the location of the telegram
            if let Some(position) = self
                .fetch_gps_position(telegram.reporting_point, telegram.region)
                .await
            {
                self.send_waypoint(GrpcWaypoint {
                    id: 0u64,
                    source: WayPointType::R09Telegram as i32,
                    region: telegram.region,
                    time: telegram.time,
                    lat: position.0,
                    lon: position.1,
                    line: telegram.line.unwrap(), //
                    run: telegram.run_number.unwrap(),
                    delayed: telegram.delay.map(convert_delay),
                    r09_reporting_point: Some(telegram.reporting_point),
                    r09_destination_number: telegram.destination_number,
                })
                .await;
            }
        }
    }

    async fn handle_gps(&mut self, point: GrpcGpsPoint) {
        debug!("handling gps {:?}", &point);
        let mut delay = None;

        match self
            .vehicles
            .entry(point.region)
            .or_insert_with(HashMap::new)
            .get_mut(&(point.line, point.run))
        {
            // vehicle was seen before
            Some(vehicle_information) => {
                vehicle_information.last_gps_update = point.time;

                // this checks if there is delay information to enrich the waypoint with
                if point.time - vehicle_information.last_r09_update < ACCEPT_DELAY {
                    delay = vehicle_information.delay;
                }
            }
            // vehicle was never seen before
            None => {
                let vehicle_information = VehicleState {
                    last_r09_update: 0,
                    last_gps_update: point.time,
                    delay: None,
                };

                self.vehicles
                    .get_mut(&point.region)
                    .unwrap()
                    .insert((point.line, point.run), vehicle_information);
            }
        }

        self.send_waypoint(GrpcWaypoint {
            id: 0u64,
            source: WayPointType::TrekkieGPS as i32,
            region: point.region,
            time: point.time,
            lat: point.lat,
            lon: point.lon,
            line: point.line,
            run: point.run,
            delayed: delay,
            r09_reporting_point: None,
            r09_destination_number: None,
        })
        .await;
    }

    /// sending out they waypoint to all the waiting servers
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
