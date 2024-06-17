use crate::{
    queue::{Queue, QueueEntry},
    TextReplyRequest, TextReplyResponse,
};
use reply_client::Client;

use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, Notify};
use tracing::{instrument, Span};

struct Shared {
    batching_task: Notify,
}

#[derive(Clone)]
pub struct Processor {
    queue: Queue,
    shared: Arc<Shared>,
}

impl Processor {
    pub fn new(client: Client) -> Self {
        let shared = Arc::new(Shared {
            batching_task: Notify::new(),
        });
        let queue = Queue::new();

        tokio::spawn(batching_task(queue.clone(), shared.clone(), client));
        Self { queue, shared }
    }

    #[instrument(skip_all)]
    pub async fn process_request(
        &self,
        request: TextReplyRequest,
    ) -> Result<mpsc::UnboundedReceiver<TextReplyResponse>> {
        let (response_tx, response_rx) = mpsc::unbounded_channel();
        self.queue.append(QueueEntry {
            request,
            response_tx,
            span: Span::current(),
            temp_span: None,
            queue_time: Instant::now(),
            batch_time: None,
        });
        self.shared.batching_task.notify_one();
        Ok(response_rx)
    }
}

async fn batching_task(queue: Queue, shared: Arc<Shared>, mut client: Client) {
    loop {
        shared.batching_task.notified().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await; // Simulate batching.
        while let Some((entries, batch, _)) = queue.next_batch().await {
            let (batch_id, _) = (batch.id.clone(), batch.size.clone());
            let batch_response = client.generate_reply(batch).await.unwrap();
            let all_responses = batch_response
                .responses
                .iter()
                .map(|r| r.message.clone())
                .collect::<Vec<String>>();

            let time = batch_response.elapsed;

            for (response, (_, entry)) in
                std::iter::zip(batch_response.responses.into_iter(), entries.into_iter())
            {
                let response = TextReplyResponse {
                    message: response.message,
                    batch_id: batch_id as u32,
                    request_id: response.request_id as u32,
                    batch_size: all_responses.len() as u32,
                    processing_time: time,
                    other_responses: all_responses.clone(),
                };
                let _ = entry.response_tx.send(response).map_err(|e| {
                    tracing::error!("Error: {:?}", e);
                });
            }
        }
    }
}
