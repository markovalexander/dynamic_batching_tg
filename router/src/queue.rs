use crate::{TextReplyRequest, TextReplyResponse};
use reply_client::{ClientBatch, HttpRequest};
use std::collections::{HashMap, VecDeque};
use tokio::sync::{mpsc, oneshot};
use tracing::{instrument, Span};

#[derive(Debug, Clone)]
pub(crate) struct QueueEntry {
    pub request: TextReplyRequest,
    /// Response sender to communicate between the Infer struct and the batching_task
    pub response_tx: mpsc::UnboundedSender<TextReplyResponse>,
}

#[derive(Debug)]
enum QueueCommand {
    Append(Box<QueueEntry>, Span),
    NextBatch {
        response_sender: oneshot::Sender<Option<NextBatch>>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Queue {
    queue_sender: mpsc::UnboundedSender<QueueCommand>,
}

type NextBatch = (HashMap<u32, QueueEntry>, ClientBatch, Span);

impl Queue {
    pub fn new() -> Self {
        let (queue_sender, queue_receiver) = mpsc::unbounded_channel();
        tokio::spawn(queue_task(queue_receiver));
        Self { queue_sender }
    }

    #[instrument(skip_all)]
    pub fn append(&self, entry: QueueEntry) {
        let command = QueueCommand::Append(Box::new(entry), Span::current());
        self.queue_sender.send(command).unwrap();
    }

    #[instrument(skip_all)]
    pub async fn next_batch(&self) -> Option<NextBatch> {
        let (response_sender, response_receiver) = oneshot::channel();
        let command = QueueCommand::NextBatch {
            response_sender,
            span: Span::current(),
        };
        self.queue_sender.send(command).unwrap();
        response_receiver.await.unwrap()
    }
}

struct QueueState {
    entries: VecDeque<(u64, QueueEntry)>,
    next_id: u64,
    next_batch_id: u64,
}

impl QueueState {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(64),
            next_id: 1,
            next_batch_id: 1,
        }
    }

    pub fn append(&mut self, entry: QueueEntry) {
        self.entries.push_back((self.next_id, entry));
    }

    #[instrument(skip_all, fields(next_id, next_batch_id))]
    pub async fn next_batch(&mut self) -> Option<NextBatch> {
        if self.entries.is_empty() {
            return None;
        }
        tracing::info!("Gathering batch from {} entries", self.entries.len());
        let mut batch_entries: HashMap<u32, QueueEntry> =
            HashMap::with_capacity(self.entries.len());

        let mut batch_requests = Vec::with_capacity(self.entries.len());

        let i = 0;
        while i < self.entries.len() {
            if let Some((_, entry)) = self.entries.get_mut(i) {
                if entry.response_tx.is_closed() {
                    // Remove the closed entry from the VecDeque
                    self.entries.remove(i);
                    continue;
                }

                let req = HttpRequest::new(self.next_id, entry.request.message.clone());
                batch_requests.push(req);
                batch_entries.insert(self.next_id as u32, entry.clone());
                self.next_id += 1;

                // Remove the processed entry from the VecDeque
                self.entries.remove(i);
            }
        }

        let batch = ClientBatch::new(
            self.next_batch_id,
            batch_requests.len() as u32,
            batch_requests,
        );
        self.next_batch_id += 1;

        Some((batch_entries, batch, Span::current()))
    }
}

async fn queue_task(mut queue_receiver: mpsc::UnboundedReceiver<QueueCommand>) {
    let mut state = QueueState::new();

    while let Some(command) = queue_receiver.recv().await {
        match command {
            QueueCommand::Append(entry, span) => {
                span.in_scope(|| state.append(*entry));
            }
            QueueCommand::NextBatch {
                response_sender,
                span: _,
            } => {
                let batch = state.next_batch().await;
                response_sender.send(batch).unwrap();
            }
        }
    }
}
