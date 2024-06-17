mod pb {
    include!("pb/mod.rs");
}

use pb::reply::v1::reply_service_client::ReplyServiceClient;
use pb::reply::v1::{Batch, ReplyRequest, ReplyResponse};

use anyhow::Result;
use tonic::transport::{Channel, Uri};

#[derive(Debug)]

pub struct HttpRequest {
    id: u64,
    message: String,
}
impl HttpRequest {
    pub fn new(id: u64, message: String) -> Self {
        Self { id, message }
    }
}

#[derive(Debug)]
pub struct ClientBatch {
    pub id: u64,
    pub size: u32,
    requests: Vec<HttpRequest>,
}

impl ClientBatch {
    pub fn new(id: u64, size: u32, requests: Vec<HttpRequest>) -> Self {
        Self { id, size, requests }
    }

    fn to_grpc_batch(&self) -> Batch {
        let mut requests = Vec::new();
        for request in &self.requests {
            requests.push(pb::reply::v1::Request {
                id: request.id,
                message: request.message.clone(),
            });
        }
        Batch {
            id: self.id,
            size: requests.len() as u32,
            requests,
        }
    }
}

#[derive(Clone)]
pub struct Client {
    stub: ReplyServiceClient<Channel>,
}

impl Client {
    pub async fn new(uri: Uri) -> Result<Self> {
        let channel = Channel::builder(uri).connect().await?;

        Ok(Self {
            stub: ReplyServiceClient::new(channel),
        })
    }

    pub async fn generate_reply(&mut self, request: ClientBatch) -> Result<ReplyResponse> {
        let batch = request.to_grpc_batch();
        let response = self.stub.reply(ReplyRequest { batch: Some(batch) }).await?;
        Ok(response.into_inner())
    }
}
