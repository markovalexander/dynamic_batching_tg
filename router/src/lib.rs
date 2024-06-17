#[derive(serde::Deserialize, Debug, Clone)]
pub struct TextReplyRequest {
    pub message: String,
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct TextReplyResponse {
    pub message: String,
    pub batch_id: u32,
    pub request_id: u32,
    pub batch_size: u32,
    pub processing_time: f32,
    pub other_responses: Vec<String>,
}
