use crate::queue::TimeQueue;

use tlms::grpc::receive_waypoint_client::ReceiveWaypointClient;
use tlms::grpc::{GrpcGpsPoint, GrpcWaypoint, R09GrpcTelegram};
use tlms::locations::{waypoint::WayPointType, TransmissionLocation};

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;
use diesel::{ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl};

use log::{debug, error, info};
use std::cmp::min;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

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
}

// TODO: maybe make those to configurable

// this constant defines the time difference is needed to switch from gps stream to r09
const DISCARD_R09_TIME: u64 = 60 * 1000;

// after which time delays should not be valid anymore
const ACCEPT_DELAY: u64 = 90 * 1000;

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
            const MAX_QUEUE_PROCESSING_TIME_SLICE: u128 = 50;

            //TODO: maybe optimize this later to remove code redudency
            
            let near_duration = min(self.gps_queue.lock().unwrap().most_recent_event(), self.r09_queue.lock().unwrap().most_recent_event());
            info!("sleeping for {:?}ms", near_duration);
            std::thread::sleep(near_duration);

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

        use tlms::schema::r09_transmission_locations::dsl::r09_transmission_locations;
        use tlms::schema::r09_transmission_locations::{
            region as db_region, reporting_point as db_reporting_point,
        };

        r09_transmission_locations
            .filter(db_region.eq(region))
            .filter(db_reporting_point.eq(reporting_point))
            .first::<TransmissionLocation>(&mut database_connection)
            .map(|row| (row.lat, row.lon))
            .ok()
    }

    async fn handle_r09(&mut self, telegram: R09GrpcTelegram) {
        info!("handleing telegram {:?}", &telegram);

        // cannot work with this data discard instantly
        if telegram.line.is_none() || telegram.run_number.is_none() {
            return;
        }

        // function that converts the r09 delay value into seconds
        let convert_delay = |enum_value| enum_value as f32 * 60f32;

        // if this telegram should be send out as a waypoint
        let send_r09;

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

        info!("sending r09 telegram: {}", send_r09);

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
                    r09_destination_number: telegram.destination_number
                })
                .await;
            }
        }
    }

    async fn handle_gps(&mut self, point: GrpcGpsPoint) {
        info!("handleing gps {:?}", &point);
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
            r09_destination_number: None
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
