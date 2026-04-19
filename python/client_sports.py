import zmq
import time

class SportsSubscriber:
    def __init__(self):
        self.context = zmq.Context()
        self.sub_socket = self.context.socket(zmq.SUB)
        self.req_socket = self.context.socket(zmq.REQ)

        self.sub_socket.connect("tcp://localhost:5555")
        self.req_socket.connect("tcp://localhost:5559")

    def get_sports_topics(self):
        self.req_socket.send_json({"action": "get_topics"})
        response = self.req_socket.recv_json()
        topics = response.get("topics", [])
        return [t for t in topics if t.endswith(":sports")]

    def subscribe_to_topics(self, topics):
        for topic in topics:
            self.sub_socket.setsockopt_string(zmq.SUBSCRIBE, topic)
            print(f"Subscribed to topic: {topic}")

    def receive_messages(self):
        while True:
            try:
                topic = self.sub_socket.recv_string(flags=zmq.NOBLOCK)
                message = self.sub_socket.recv_json()
                print(f"Received on topic '{topic}': {message}")
            except zmq.Again:
                pass
            time.sleep(0.1)

    def run(self):
        while True:
            topics = self.get_sports_topics()
            print("Sports topics:", topics)
            self.subscribe_to_topics(topics)

            start_time = time.time()
            while time.time() - start_time < 10:
                self.receive_messages()

if __name__ == "__main__":
    subscriber = SportsSubscriber()
    subscriber.run()
