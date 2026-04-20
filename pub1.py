import zmq
import time
import json
import uuid
import threading

class Publisher:
    def __init__(self, topics):
        self.context = zmq.Context()
        self.pub_socket = self.context.socket(zmq.PUB)
        self.ping_socket = self.context.socket(zmq.DEALER)
        self.id = str(uuid.uuid4())
        self.topics = topics

        self.pub_socket.connect("tcp://localhost:5556")
        self.ping_socket.setsockopt_string(zmq.IDENTITY, self.id)
        self.ping_socket.connect("tcp://localhost:5558")

    def register(self):
        time.sleep(0.5)  # allow connection to establish before sending
        register_msg = {
            "action": "register",
            "publisher_id": self.id,
            "topics": self.topics
        }
        self.pub_socket.send_json(register_msg)
        print(f"Registered with broker. Publisher ID: {self.id}")

    def publish(self):
        counter = 0
        while True:
            for topic in self.topics:
                topic_str = f"{self.id}:{topic}"
                data = {"message": f"Message {counter} for {topic}"}
                self.pub_socket.send_string(topic_str, zmq.SNDMORE)
                self.pub_socket.send_json(data)
                print(f"Published [{topic_str}]: {data}")
            counter += 1
            time.sleep(1)

    def handle_pings(self):
        while True:
            try:
                frames = self.ping_socket.recv_multipart(flags=zmq.NOBLOCK)
                data = json.loads(frames[-1])
                if data["action"] == "ping" and data["publisher_id"] == self.id:
                    pong_msg = {
                        "action": "pong",
                        "publisher_id": self.id
                    }
                    self.ping_socket.send_json(pong_msg)
                    print("Sent pong response")
            except zmq.Again:
                pass
            time.sleep(0.1)

    def run(self):
        self.register()
        threading.Thread(target=self.handle_pings, daemon=True).start()
        self.publish()

if __name__ == "__main__":
    topics = ["news", "weather"]
    publisher = Publisher(topics)
    publisher.run()