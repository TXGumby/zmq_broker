use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};

enum ZmqCommand {
    GetTopics { reply: oneshot::Sender<String> },
    Publish { topic: String, message: String },
}

#[derive(Clone)]
struct AppState {
    msg_tx: broadcast::Sender<String>,
    cmd_tx: Arc<mpsc::UnboundedSender<ZmqCommand>>,
}

#[tokio::main]
async fn main() {
    let (msg_tx, _) = broadcast::channel::<String>(256);
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<ZmqCommand>();

    // ZMQ subscriber thread: receives all published messages, broadcasts to WS clients
    {
        let msg_tx = msg_tx.clone();
        std::thread::spawn(move || {
            let ctx = zmq::Context::new();
            let sub = ctx.socket(zmq::SUB).unwrap();
            sub.connect("tcp://localhost:5555").unwrap();
            sub.set_subscribe(b"").unwrap();
            loop {
                if let Ok(frames) = sub.recv_multipart(0) {
                    if frames.len() == 2 {
                        let topic = String::from_utf8_lossy(&frames[0]).to_string();
                        let data_str = String::from_utf8_lossy(&frames[1]).to_string();
                        let data = serde_json::from_str::<serde_json::Value>(&data_str)
                            .unwrap_or(serde_json::Value::String(data_str));
                        let out = serde_json::json!({
                            "type": "message",
                            "topic": topic,
                            "data": data,
                        });
                        let _ = msg_tx.send(out.to_string());
                    }
                }
            }
        });
    }

    // ZMQ command thread: handles get_topics and publish requests from the browser
    std::thread::spawn(move || {
        let ctx = zmq::Context::new();

        let req = ctx.socket(zmq::REQ).unwrap();
        req.set_rcvtimeo(3000).unwrap();
        req.connect("tcp://localhost:5559").unwrap();

        let pub_sock = ctx.socket(zmq::PUB).unwrap();
        pub_sock.connect("tcp://localhost:5556").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(500));

        let mut cmd_rx = cmd_rx;
        while let Some(cmd) = cmd_rx.blocking_recv() {
            match cmd {
                ZmqCommand::GetTopics { reply } => {
                    let request = serde_json::json!({ "action": "get_topics" });
                    let _ = req.send(request.to_string().as_bytes(), 0);
                    let topics = match req.recv_msg(0) {
                        Ok(msg) => {
                            if let Ok(s) = std::str::from_utf8(&msg) {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                                    v["topics"].clone()
                                } else {
                                    serde_json::json!([])
                                }
                            } else {
                                serde_json::json!([])
                            }
                        }
                        Err(_) => serde_json::json!([]),
                    };
                    let out = serde_json::json!({ "type": "topics", "topics": topics });
                    let _ = reply.send(out.to_string());
                }
                ZmqCommand::Publish { topic, message } => {
                    let _ = pub_sock.send(topic.as_bytes(), zmq::SNDMORE);
                    let data = serde_json::json!({ "message": message });
                    let _ = pub_sock.send(data.to_string().as_bytes(), 0);
                }
            }
        }
    });

    let state = AppState {
        msg_tx,
        cmd_tx: Arc::new(cmd_tx),
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Web client running at http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut zmq_rx = state.msg_tx.subscribe();

    // Per-client channel for direct replies (e.g. topic list response)
    let (direct_tx, mut direct_rx) = mpsc::unbounded_channel::<String>();

    // Task: forward ZMQ broadcast messages + direct replies to the WebSocket client
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = zmq_rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if ws_tx.send(Message::Text(msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
                Some(msg) = direct_rx.recv() => {
                    if ws_tx.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Task: receive commands from browser, forward to ZMQ command thread
    let cmd_tx = state.cmd_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = ws_rx.next().await {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                match v["action"].as_str() {
                    Some("get_topics") => {
                        let (reply_tx, reply_rx) = oneshot::channel();
                        if cmd_tx
                            .send(ZmqCommand::GetTopics { reply: reply_tx })
                            .is_ok()
                        {
                            if let Ok(resp) = reply_rx.await {
                                let _ = direct_tx.send(resp);
                            }
                        }
                    }
                    Some("publish") => {
                        if let (Some(topic), Some(message)) =
                            (v["topic"].as_str(), v["message"].as_str())
                        {
                            let _ = cmd_tx.send(ZmqCommand::Publish {
                                topic: topic.to_string(),
                                message: message.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
