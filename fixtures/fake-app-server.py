import json
import sys

pending_turn = None
turn_counter = 0

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
        turn_counter += 1
        pending_turn = f"turn-fake-{turn_counter}"
        send({"id": msg["id"], "result": {"turn": {"id": pending_turn}}})
        text = msg["params"]["input"][0]["text"]
        if "fail-me" in text:
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "failed", "error": {"message": "structured output rejected", "additionalDetails": "schema mismatch"}}}})
            pending_turn = None
        elif "cancel-me" not in text:
            if "outputSchema" in msg["params"]:
                if "invalid-structured" in text:
                    answer = json.dumps({"answer_markdown": "missing fields"}, separators=(",", ":"))
                else:
                    answer = json.dumps({
                        "answer_markdown": "结构化回答 [1]",
                        "citations": [{
                            "id": "1", "paper_id": "paper:one", "revision": "revision-one", "page": 1,
                            "section": None, "locator": None, "quote": "evidence", "prefix": "", "suffix": "",
                            "explanation": "supports the answer"
                        }],
                        "annotation_intents": []
                    }, ensure_ascii=False, separators=(",", ":"))
            else:
                answer = "structured answer"
            midpoint = max(1, len(answer) // 2)
            send({"method": "item/agentMessage/delta", "params": {"threadId": msg["params"]["threadId"], "turnId": pending_turn, "itemId": "item-1", "delta": answer[:midpoint]}})
            send({"method": "item/agentMessage/delta", "params": {"threadId": msg["params"]["threadId"], "turnId": pending_turn, "itemId": "item-1", "delta": answer[midpoint:]}})
            send({"method": "item/completed", "params": {"threadId": msg["params"]["threadId"], "turnId": pending_turn, "item": {"id": "item-1", "type": "agentMessage", "text": answer}}})
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "completed"}}})
            pending_turn = None
    elif method == "turn/interrupt":
        send({"id": msg["id"], "result": {}})
        send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn or "turn-fake-unknown", "items": [], "status": "interrupted"}}})
        pending_turn = None
    elif "id" in msg:
        send({"id": msg["id"], "error": {"code": -32601, "message": "unknown method"}})
