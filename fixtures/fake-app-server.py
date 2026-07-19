import json
import sys

pending_turn = None

def send(value):
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()

for raw in sys.stdin:
    msg = json.loads(raw)
    method = msg.get("method")
    if method == "initialize":
        send({"id": msg["id"], "result": {"userAgent": "fake", "platformFamily": "unix", "platformOs": "linux"}})
    elif method == "initialized":
        continue
    elif method == "thread/start":
        send({"id": msg["id"], "result": {"thread": {"id": "thread-fake"}}})
    elif method == "thread/resume":
        send({"id": msg["id"], "result": {"thread": {"id": msg["params"]["threadId"]}}})
    elif method == "turn/start":
        pending_turn = "turn-fake"
        send({"id": msg["id"], "result": {"turn": {"id": pending_turn}}})
        text = msg["params"]["input"][0]["text"]
        if "fail-me" in text:
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "failed", "error": {"message": "structured output rejected", "additionalDetails": "schema mismatch"}}}})
            pending_turn = None
        elif "cancel-me" not in text:
            send({"method": "item/agentMessage/delta", "params": {"threadId": msg["params"]["threadId"], "turnId": pending_turn, "itemId": "item-1", "delta": "structured "}})
            send({"method": "item/completed", "params": {"threadId": msg["params"]["threadId"], "turnId": pending_turn, "item": {"id": "item-1", "type": "agentMessage", "text": "structured answer"}}})
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "completed"}}})
            pending_turn = None
    elif method == "turn/interrupt":
        send({"id": msg["id"], "result": {}})
        send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn or "turn-fake", "items": [], "status": "interrupted"}}})
        pending_turn = None
    elif "id" in msg:
        send({"id": msg["id"], "error": {"code": -32601, "message": "unknown method"}})
