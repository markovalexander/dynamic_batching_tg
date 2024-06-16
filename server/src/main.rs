mod pb {
    include!("pb/mod.rs");
    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("my_descriptor");
}

use clap::Parser;
use pb::reply::v1::reply_service_server::{ReplyService, ReplyServiceServer};
use pb::reply::v1::{ReplyRequest, ReplyResponse};
use tonic::transport::Server;

use anyhow::Result;
use std::time::Instant;
use tonic::{Request, Response, Status};
use tracing::instrument;

struct MyReplyService;

#[tonic::async_trait]
impl ReplyService for MyReplyService {
    #[instrument(skip_all, fields(elapsed_time, batch_size, batch_id))]
    async fn reply(
        &self,
        request: Request<ReplyRequest>,
    ) -> Result<Response<ReplyResponse>, Status> {
        let span = tracing::Span::current();
        let start_time = Instant::now();
        let batch = request.into_inner().batch.unwrap();

        let mut responses = vec![];
        for request in batch.requests.iter() {
            responses.push(pb::reply::v1::Response {
                request_id: request.id,
                message: format!("Response for [{}]", request.message),
            });
        }

        let elapsed_time = start_time.elapsed().as_secs_f32();

        span.record("elapsed_time", elapsed_time);
        span.record("batch_size", batch.size);
        span.record("batch_id", batch.id);

        let reply = ReplyResponse {
            responses,
            elapsed: elapsed_time,
        };
        tracing::info!("SUCCESS");
        Ok(Response::new(reply))
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value = "50051")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    tracing::info!("Starting gRPC server with Args={:?}", args);

    let addr = format!("[::]:{}", args.port).parse()?;

    let reply_service = MyReplyService {};
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(pb::FILE_DESCRIPTOR_SET)
        .build()?;

    Server::builder()
        .add_service(ReplyServiceServer::new(reply_service))
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}
