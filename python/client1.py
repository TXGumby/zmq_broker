import zmq
import json
import time

class Subscriber:
    def __init__(self):
        self.context = zmq.Context()
        self.sub_socket = self.context.socket(zmq.SUB)
        self.req_socket = self.context.socket(zmq.REQ)

        self.sub_socket.connect("tcp://localhost:5555")
        self.req_socket.connect("tcp://localhost:5559")

    def get_topic_list(self):
        request = {
            "action": "get_topics"
        }
        self.req_socket.send_json(request)
        response = self.req_socket.recv_json()
        return response.get("topics", [])

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
            topics = self.get_topic_list()
            print("Available topics:", topics)
            self.subscribe_to_topics(topics)

            # Receive messages for a while before refreshing the topic list
            start_time = time.time()
            while time.time() - start_time < 10:  # Refresh every 10 seconds
                self.receive_messages()

if __name__ == "__main__":
    subscriber = Subscriber()
    subscriber.run()