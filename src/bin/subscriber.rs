use std::env;
use std::time::{Duration, Instant};
use zmq_broker::TopicListResponse;

fn get_topics(req_socket: &zmq::Socket) -> Vec<String> {
    let request = serde_json::json!({ "action": "get_topics" });
    if let Err(e) = req_socket.send(request.to_string().as_bytes(), 0) {
        eprintln!("Topic request send error: {}", e);
        return vec![];
    }
    match req_socket.recv_msg(0) {
        Ok(msg) => {
            if let Ok(json_str) = std::str::from_utf8(&msg) {
                if let Ok(resp) = serde_json::from_str::<TopicListResponse>(json_str) {
                    return resp.topics;
                }
            }
            vec![]
        }
        Err(_) => {
            eprintln!("Timeout waiting for topic list from broker");
            vec![]
        }
    }
}

fn main() {
    let filters: Vec<String> = env::args().skip(1).collect();
    if filters.is_empty() {
        println!("Subscribing to all topics");
    } else {
        println!("Filtering to topics: {:?}", filters);
    }

    let context = zmq::Context::new();

    let req_socket = context.socket(zmq::REQ).unwrap();
    req_socket.set_rcvtimeo(3000).unwrap(); // 3s timeout so REQ never gets stuck
    req_socket.connect("tcp://localhost:5559").unwrap();

    let sub_socket = context.socket(zmq::SUB).unwrap();
    sub_socket.connect("tcp://localhost:5555").unwrap();

    let mut subscribed: Vec<String> = vec![];
    let mut last_refresh = Instant::now() - Duration::from_secs(11); // trigger immediately

    loop {
        // Refresh topic list every 10 seconds
        if last_refresh.elapsed() > Duration::from_secs(10) {
            let all_topics = get_topics(&req_socket);

            let matched: Vec<String> = if filters.is_empty() {
                all_topics
            } else {
                all_topics
                    .into_iter()
                    .filter(|t| filters.iter().any(|f| t.ends_with(&format!(":{}", f))))
                    .collect()
            };

            // Subscribe to any new topics
            for topic in &matched {
                if !subscribed.contains(topic) {
                    match sub_socket.set_subscribe(topic.as_bytes()) {
                        Ok(()) => {
                            println!("Subscribed to: {}", topic);
                            subscribed.push(topic.clone());
                        }
                        Err(e) => eprintln!("Subscribe error for {}: {}", topic, e),
                    }
                }
            }

            if matched.is_empty() {
                println!("No matching topics found, retrying...");
            }

            last_refresh = Instant::now();
        }

        // Receive messages with a short timeout so we can refresh topics periodically
        match sub_socket.poll(zmq::POLLIN, 500) {
            Ok(n) if n > 0 => match sub_socket.recv_multipart(0) {
                Ok(frames) => {
                    if frames.len() == 2 {
                        let topic = String::from_utf8_lossy(&frames[0]);
                        let data = String::from_utf8_lossy(&frames[1]);
                        println!("Received [{}]: {}", topic, data);
                    }
                }
                Err(e) => eprintln!("Recv error: {}", e),
            },
            _ => {}
        }
    }
}
