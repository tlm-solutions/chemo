extern crate env_logger;

#[deny(missing_docs)]
mod queue;
mod state;

use queue::TimeQueue;
use state::{QueueGps, QueueR09, State};

//use tlms::grpc::chemo::ReceivesTelegramsClient;
use tlms::grpc::chemo_server::{Chemo, ChemoServer};
use tlms::grpc::{GrpcGpsPoint, R09GrpcTelegram, ReturnCode};

use std::env;
use std::sync::{Arc, Mutex};

use log::info;
use tonic::{transport::Server, Request, Response, Status};

#[derive(Clone)]
pub struct DataReceiver {
    r09_queue: QueueR09,
    gps_queue: QueueGps,
}

impl DataReceiver {
    fn new(r09_queue: QueueR09, gps_queue: QueueGps) -> DataReceiver {
        DataReceiver {
            r09_queue,
            gps_queue,
        }
    }
}

#[tonic::async_trait]
impl Chemo for DataReceiver {
    async fn receive_r09(
        &self,
        request: Request<R09GrpcTelegram>,
    ) -> Result<Response<ReturnCode>, Status> {
        let extracted = request.into_inner();

        if let Ok(mut queue) = self.r09_queue.lock() {
            queue.insert(extracted);
        }

        Ok(Response::new(ReturnCode { status: 0 }))
    }
    async fn receive_gps(
        &self,
        request: Request<GrpcGpsPoint>,
    ) -> Result<Response<ReturnCode>, Status> {
        let extracted = request.into_inner();

        if let Ok(mut queue) = self.gps_queue.lock() {
            queue.insert(extracted);
        }

        Ok(Response::new(ReturnCode { status: 0 }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    info!("Starting Chemo Therapy ... inserting the catheter");

    let r09_queue = Arc::new(Mutex::new(TimeQueue::new()));
    let gps_queue = Arc::new(Mutex::new(TimeQueue::new()));

    let default_grpc_chemo_host = String::from("127.0.0.1:50051");
    let grpc_chemo_host = env::var("CHEMO_HOST")
        .unwrap_or(default_grpc_chemo_host)
        .parse()?;

    let chemo = DataReceiver::new(r09_queue.clone(), gps_queue.clone());

    Server::builder()
        .add_service(ChemoServer::new(chemo))
        .serve(grpc_chemo_host); // .await

    let mut state = State::new(r09_queue, gps_queue);
    state.processing_loop().await;

    Ok(())
}
