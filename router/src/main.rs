mod processor;
mod queue;

use anyhow::Result;
use axum::{http::StatusCode, routing::post, Json, Router};
use clap::Parser;

use tokio::net::TcpListener;
use tonic::transport::Uri;

use router::{TextReplyRequest, TextReplyResponse};

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long, default_value = "127.0.0.1:8080")]
    address: String,
    #[clap(short, long, default_value = "127.0.0.1:50051")]
    grpc_address: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    tracing::info!("args: {:?}", &args);

    let uri = Uri::builder()
        .scheme("http")
        .authority(args.grpc_address.clone())
        .path_and_query("/")
        .build()
        .unwrap();
    let client = reply_client::Client::new(uri.clone()).await?;
    let proc = processor::Processor::new(client);

    tracing::info!("Connected to gRPC server at {}", args.grpc_address);

    let app = Router::new().route(
        "/process_message",
        post(move |echo_request: Json<TextReplyRequest>| {
            message_handler(echo_request, proc.clone())
        }),
    );

    tracing::info!("Listening on {}", &args.address);
    let listener = TcpListener::bind(&args.address).await.unwrap();

    axum::serve(listener, app).await.unwrap();
    tracing::info!("Server shutdown");

    Ok(())
}

async fn message_handler(
    request: Json<TextReplyRequest>,
    processor: processor::Processor,
) -> Result<Json<TextReplyResponse>, StatusCode> {
    let request = request.0;

    tracing::info!("Processing request: {:?}", &request);

    let mut response_rx = processor.process_request(request).await.map_err(|e| {
        tracing::error!("Error: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let response = match response_rx.recv().await {
        Some(response) => response,
        None => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let json_response = TextReplyResponse {
        message: response.message,
        batch_id: response.batch_id,
        request_id: response.request_id,
        batch_size: response.batch_id,
        processing_time: response.processing_time,
        other_responses: response.other_responses,
    };

    Ok(Json(json_response))
}
