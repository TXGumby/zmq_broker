use std::env;
use std::thread;
use std::time::Duration;
use zmq_broker::{PingMessage, PongMessage, RegisterMessage};

fn main() {
    let topics: Vec<String> = env::args().skip(1).collect();
    if topics.is_empty() {
        eprintln!("Usage: publisher <topic1> [topic2] ...");
        std::process::exit(1);
    }

    let id = uuid::Uuid::new_v4().to_string();
    println!("Publisher ID: {}", id);
    println!("Topics: {:?}", topics);

    let context = zmq::Context::new();

    // PUB socket for publishing messages and registration
    let pub_socket = context.socket(zmq::PUB).unwrap();
    pub_socket.connect("tcp://localhost:5556").unwrap();

    // DEALER socket for ping/pong, identity set to publisher UUID
    let ping_socket = context.socket(zmq::DEALER).unwrap();
    ping_socket.set_identity(id.as_bytes()).unwrap();
    ping_socket.connect("tcp://localhost:5558").unwrap();

    // Allow connections to establish before sending register
    thread::sleep(Duration::from_millis(500));

    // Register with broker
    let register_msg = RegisterMessage {
        action: "register".to_string(),
        publisher_id: id.clone(),
        topics: topics.clone(),
    };
    let register_json = serde_json::to_string(&register_msg).unwrap();
    pub_socket.send(register_json.as_bytes(), 0).unwrap();
    println!("Registered with broker.");

    // Spawn ping/pong handler thread
    let id_clone = id.clone();
    thread::spawn(move || loop {
        match ping_socket.recv_msg(0) {
            Ok(msg) => {
                if let Ok(json_str) = std::str::from_utf8(&msg) {
                    if let Ok(ping) = serde_json::from_str::<PingMessage>(json_str) {
                        if ping.action == "ping" && ping.publisher_id == id_clone {
                            let pong = PongMessage {
                                action: "pong".to_string(),
                                publisher_id: id_clone.clone(),
                            };
                            match serde_json::to_string(&pong) {
                                Ok(pong_json) => {
                                    if let Err(e) =
                                        ping_socket.send(pong_json.as_bytes(), 0)
                                    {
                                        eprintln!("Pong send error: {}", e);
                                    } else {
                                        println!("Sent pong");
                                    }
                                }
                                Err(e) => eprintln!("Pong serialize error: {}", e),
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("Ping recv error: {}", e),
        }
    });

    // Publish loop
    let mut counter: u64 = 0;
    loop {
        for topic in &topics {
            let topic_str = format!("{}:{}", id, topic);
            let data = serde_json::json!({ "message": format!("Message {} for {}", counter, topic) });
            if let Err(e) = pub_socket.send(topic_str.as_bytes(), zmq::SNDMORE) {
                eprintln!("Publish topic send error: {}", e);
                continue;
            }
            if let Err(e) = pub_socket.send(data.to_string().as_bytes(), 0) {
                eprintln!("Publish data send error: {}", e);
                continue;
            }
            println!("Published [{}]: {}", topic_str, data);
        }
        counter += 1;
        thread::sleep(Duration::from_secs(1));
    }
}
