use anyhow::Result;
use clap::Parser;
use std::fmt::Debug;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::instrument;

#[derive(Parser, Debug)]
#[clap(author = "Alex Markov", version = "0.1.0", about = "Simple ")]
struct Args {
    #[clap(short, long, default_value = "127.0.0.1:8080")]
    reply_server_address: String,
    #[clap(short, long)]
    tg_token: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct HttpResponse {
    message: String,
    batch_id: u32,
    request_id: u32,
    batch_size: u32,
    processing_time: f32,
    other_responses: Vec<String>,
}
impl HttpResponse {
    pub fn to_message(&self) -> String {
        let meta = format!(
            "Meta: [ Batch ID: {}\nRequest ID: {}\nBatch Size: {}\nProcessing Time: {}\nAll Responses: {:?} ]",
            self.batch_id, self.request_id, self.batch_size, self.processing_time, self.other_responses
        );
        let msg = format!("{}\n{}", self.message, meta);
        tracing::debug!("Message: {}", msg);
        msg
    }
}

struct HttpClient {
    client: reqwest::Client,
    url: String,
}
impl Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient")
            .field("url", &self.url)
            .finish()
    }
}

impl HttpClient {
    fn new(url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: format!("http://{}/process_message", url.to_owned()),
        }
    }
    async fn send_request(&self, json_data: serde_json::Value) -> Result<HttpResponse> {
        let text_response = self.client.post(&self.url).json(&json_data).send().await?;

        let response = text_response.json().await?;
        Ok(response)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args = Args::parse();
    tracing::info!(
        "Starting TG bot connected to HTTP server on [{:?}]",
        args.reply_server_address
    );

    let bot = match args.tg_token {
        Some(token) => Bot::new(token),
        None => {
            tracing::warn!("Telegram token is not provided, creating from env");
            Bot::from_env()
        }
    };
    let bot_info = bot.get_me().await.unwrap();
    tracing::info!("{}", format!("Started bot: {:?}", bot_info.user));

    let client = Arc::new(HttpClient::new(&args.reply_server_address));

    teloxide::repl(bot, move |bot: Bot, msg: Message| {
        let client = client.clone();
        async move {
            let message = bot_msg_handler(bot, msg, client).await;
            match message {
                Ok(_) => {}
                Err(e) => tracing::error!("Error sending message: {:?}", e),
            }
            Ok(())
        }
    })
    .await;
    Ok(())
}

#[instrument(skip_all, fields(user_text, reply_text, reply_id, user))]
async fn bot_msg_handler(bot: Bot, msg: Message, client: Arc<HttpClient>) -> Result<Message> {
    let span = tracing::Span::current();
    let text = msg.text().unwrap();

    let response = client
        .send_request(serde_json::json!({"message": text}))
        .await?;

    let reply_msg = bot
        .send_message(msg.chat.id, response.to_message())
        .send()
        .await?;

    span.record("reply_text", reply_msg.text());
    span.record("reply_id", reply_msg.id.0);

    let user = msg.from().unwrap().username.clone();

    span.record("user", user);
    span.record("user_text", text);

    tracing::info!("SUCCESS");
    Ok(msg)
}
