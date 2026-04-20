use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use zmq::{Context, Socket};
use zmq_broker::{PingMessage, PongMessage, RegisterMessage, TopicListResponse};

// Struct to store publisher information
#[derive(Clone)]
struct PublisherInfo {
    topics: Vec<String>,
    last_pong: Instant,
}

// Main broker struct
struct Broker {
    publishers: Arc<RwLock<HashMap<String, PublisherInfo>>>,
    xpub_socket: Socket,
    xsub_socket: Socket,
    rep_socket: Socket,
}

impl Broker {
    fn new(context: &Context) -> Result<Self, zmq::Error> {
        let xpub_socket = context.socket(zmq::XPUB)?;
        let xsub_socket = context.socket(zmq::XSUB)?;
        let rep_socket = context.socket(zmq::REP)?;

        xpub_socket.bind("tcp://*:5555")?;
        xsub_socket.bind("tcp://*:5556")?;
        rep_socket.bind("tcp://*:5559")?;

        Ok(Broker {
            publishers: Arc::new(RwLock::new(HashMap::new())),
            xpub_socket,
            xsub_socket,
            rep_socket,
        })
    }

    fn handle_register(&self, msg: RegisterMessage) {
        let mut publishers = self.publishers.write();
        publishers.insert(
            msg.publisher_id.clone(),
            PublisherInfo {
                topics: msg.topics,
                last_pong: Instant::now(),
            },
        );
    }

    fn handle_topic_list_request(&self) -> TopicListResponse {
        let publishers = self.publishers.read();
        let mut topics = Vec::new();
        for (publisher_id, info) in publishers.iter() {
            for topic in &info.topics {
                topics.push(format!("{}:{}", publisher_id, topic));
            }
        }
        TopicListResponse {
            action: "topic_list_response".to_string(),
            topics,
        }
    }

    fn remove_inactive_publishers(&self) {
        let mut publishers = self.publishers.write();
        publishers.retain(|_, info| info.last_pong.elapsed() < Duration::from_secs(60));
    }

    fn run_ping_mechanism(&self, context: &Context) {
        let publishers = Arc::clone(&self.publishers);
        let context = context.clone();

        thread::spawn(move || {
            let ping_socket = context.socket(zmq::ROUTER).unwrap();
            ping_socket.bind("tcp://*:5558").unwrap();

            let mut last_ping = Instant::now();

            loop {
                let mut items = [ping_socket.as_poll_item(zmq::POLLIN)];
                let rc = zmq::poll(&mut items, Duration::from_millis(100).as_millis() as i64);

                if rc.is_ok() && items[0].is_readable() {
                    if let Ok(msg) = ping_socket.recv_multipart(0) {
                        if msg.len() == 2 {
                            if let Ok(pong_msg) = serde_json::from_slice::<PongMessage>(&msg[1]) {
                                if pong_msg.action == "pong" {
                                    println!("Received pong from: {}", &pong_msg.publisher_id[..8]);
                                    let mut publishers = publishers.write();
                                    if let Some(info) = publishers.get_mut(&pong_msg.publisher_id) {
                                        info.last_pong = Instant::now();
                                    }
                                }
                            }
                        }
                    }
                }

                if last_ping.elapsed() > Duration::from_secs(20) {
                    let publishers_clone = publishers.read().clone();
                    for (id, _) in publishers_clone.iter() {
                        let ping_msg = PingMessage {
                            action: "ping".to_string(),
                            publisher_id: id.clone(),
                        };
                        let ping_json = serde_json::to_string(&ping_msg).unwrap();
                        let _ = ping_socket.send_multipart(&[id.as_bytes(), ping_json.as_bytes()], 0);
                    }
                    last_ping = Instant::now();
                }

                thread::sleep(Duration::from_millis(100));
            }
        });
    }

    fn run(&mut self, context: &Context) -> Result<(), zmq::Error> {
        // Subscribe XSUB to all topics so it receives everything from connected PUBs
        self.xsub_socket.send(b"\x01".as_ref(), 0)?;

        self.run_ping_mechanism(context);

        let mut items = [
            self.xpub_socket.as_poll_item(zmq::POLLIN),
            self.xsub_socket.as_poll_item(zmq::POLLIN),
            self.rep_socket.as_poll_item(zmq::POLLIN),
        ];

        loop {
            zmq::poll(&mut items, Duration::from_millis(1000).as_millis() as i64)?;

            // XPUB readable: subscription notifications from subscribers
            if items[0].is_readable() {
                let msg = self.xpub_socket.recv_msg(zmq::DONTWAIT)?;
                let msg_vec = msg.to_vec();
                if msg_vec.len() > 0 && (msg_vec[0] == 1 || msg_vec[0] == 0) {
                    let topic = String::from_utf8_lossy(&msg_vec[1..]).to_string();
                    println!("Subscription change: {}", topic);
                    let _ = self.xsub_socket.send(&msg_vec, 0);
                }
            }

            // XSUB readable: messages from publishers
            if items[1].is_readable() {
                let frames = self.xsub_socket.recv_multipart(zmq::DONTWAIT)?;
                if frames.len() == 1 {
                    // single-frame: could be a register message
                    if let Ok(json_str) = std::str::from_utf8(&frames[0]) {
                        if let Ok(register_msg) = serde_json::from_str::<RegisterMessage>(json_str) {
                            if register_msg.action == "register" {
                                println!("Registered publisher: {}", register_msg.publisher_id);
                                self.handle_register(register_msg);
                                continue;
                            }
                        }
                    }
                    let _ = self.xpub_socket.send(&frames[0], 0);
                } else {
                    // multipart: topic frame + data frame(s), forward as-is
                    let last = frames.len() - 1;
                    for (i, frame) in frames.iter().enumerate() {
                        let flags = if i < last { zmq::SNDMORE } else { 0 };
                        let _ = self.xpub_socket.send(frame.as_slice(), flags);
                    }
                }
            }

            // REP readable: topic list requests from subscribers
            if items[2].is_readable() {
                if let Ok(_req) = self.rep_socket.recv_msg(zmq::DONTWAIT) {
                    let response = self.handle_topic_list_request();
                    let response_json = serde_json::to_string(&response).unwrap();
                    println!("Sending topic list: {:?}", response.topics);
                    let _ = self.rep_socket.send(response_json.as_bytes(), 0);
                }
            }

            self.remove_inactive_publishers();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::new();
    let mut broker = Broker::new(&context)?;
    broker.run(&context)?;
    Ok(())
}
