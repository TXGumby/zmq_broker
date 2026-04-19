use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct RegisterMessage {
    pub action: String,
    pub publisher_id: String,
    pub topics: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PingMessage {
    pub action: String,
    pub publisher_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct PongMessage {
    pub action: String,
    pub publisher_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct TopicListRequest {
    pub action: String,
}

#[derive(Serialize, Deserialize)]
pub struct TopicListResponse {
    pub action: String,
    pub topics: Vec<String>,
}
