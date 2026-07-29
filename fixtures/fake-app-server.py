import json
import os
import sys

pending_turn = None
pending_server_request = None
turn_counter = 0
active_dynamic_tools = []
reject_dynamic_tools = "--reject-dynamic-tools" in sys.argv

def send(value):
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()

for raw in sys.stdin:
    msg = json.loads(raw)
    method = msg.get("method")
    if method is None and pending_server_request == "dynamic-tool" and msg.get("id") == 900:
        assert msg["result"]["success"] is True
        assert msg["result"]["contentItems"][0]["type"] == "inputText"
        assert "rule complexity" in msg["result"]["contentItems"][0]["text"]
        send({"method": "item/completed", "params": {"threadId": "thread-fake", "turnId": pending_turn, "item": {
            "id": "call-1", "type": "dynamicToolCall", "tool": "research_search",
            "arguments": {"query": "rule complexity", "reason": "find prior work"},
            "status": "completed", "success": True, "contentItems": msg["result"]["contentItems"]
        }}})
        answer = "tool-backed answer"
        send({"method": "item/agentMessage/delta", "params": {"threadId": "thread-fake", "turnId": pending_turn, "itemId": "item-1", "delta": answer}})
        send({"method": "item/completed", "params": {"threadId": "thread-fake", "turnId": pending_turn, "item": {"id": "item-1", "type": "agentMessage", "text": answer}}})
        send({"method": "turn/completed", "params": {"threadId": "thread-fake", "turn": {"id": pending_turn, "items": [], "status": "completed"}}})
        pending_server_request = None
        pending_turn = None
        continue
    if method is None and pending_server_request == "approval" and msg.get("id") == 901:
        assert msg["error"]["code"] == -32000
        answer = "approval denied safely"
        send({"method": "item/completed", "params": {"threadId": "thread-fake", "turnId": pending_turn, "item": {"id": "item-1", "type": "agentMessage", "text": answer}}})
        send({"method": "turn/completed", "params": {"threadId": "thread-fake", "turn": {"id": pending_turn, "items": [], "status": "completed"}}})
        pending_server_request = None
        pending_turn = None
        continue
    if method == "initialize":
        send({"id": msg["id"], "result": {"userAgent": "fake", "platformFamily": "unix", "platformOs": "linux"}})
    elif method == "initialized":
        continue
    elif method == "model/list":
        send({"id": msg["id"], "result": {"data": [{
            "id": "gpt-test-id", "model": "gpt-test", "displayName": "GPT Test",
            "description": "test model", "hidden": False, "isDefault": True,
            "defaultReasoningEffort": "low",
            "supportedReasoningEfforts": [
                {"effort": "low", "description": "fast"},
                {"effort": "high", "description": "deep"}
            ],
            "serviceTiers": [{"id": "priority", "name": "Fast", "description": "fast"}]
        }]}})
    elif method == "thread/start":
        if reject_dynamic_tools and "dynamicTools" in msg["params"]:
            send({"id": msg["id"], "error": {"code": -32602, "message": "dynamicTools unsupported"}})
            continue
        active_dynamic_tools = msg["params"].get("dynamicTools", [])
        send({"id": msg["id"], "result": {"thread": {"id": "thread-fake"}}})
    elif method == "thread/resume":
        if reject_dynamic_tools and "dynamicTools" in msg["params"]:
            send({"id": msg["id"], "error": {"code": -32602, "message": "dynamicTools unsupported"}})
            continue
        active_dynamic_tools = msg["params"].get("dynamicTools", [])
        send({"id": msg["id"], "result": {"thread": {"id": msg["params"]["threadId"]}}})
    elif method == "turn/start":
        turn_counter += 1
        pending_turn = f"turn-fake-{turn_counter}"
        send({"id": msg["id"], "result": {"turn": {"id": pending_turn}}})
        text = msg["params"]["input"][0]["text"]
        if "runtime-tmp" in text:
            send({"method": "test/runtime-tmp", "params": {"path": os.environ.get("TMPDIR")}})
        if "settings" in text:
            send({"method": "test/turn-params", "params": msg["params"]})
        if "fail-me" in text:
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "failed", "error": {"message": "structured output rejected", "additionalDetails": "schema mismatch"}}}})
            pending_turn = None
        elif "call-research-search" in text:
            assert any(tool.get("name") == "research_search" for tool in active_dynamic_tools)
            send({"method": "item/started", "params": {"threadId": msg["params"]["threadId"], "turnId": pending_turn, "item": {
                "id": "call-1", "type": "dynamicToolCall", "tool": "research_search",
                "arguments": {"query": "rule complexity", "reason": "find prior work"},
                "status": "inProgress"
            }}})
            send({"method": "item/tool/call", "id": 900, "params": {
                "threadId": msg["params"]["threadId"], "turnId": pending_turn,
                "callId": "call-1", "tool": "research_search",
                "arguments": {"query": "rule complexity", "reason": "find prior work"}
            }})
            pending_server_request = "dynamic-tool"
        elif "request-approval" in text:
            send({"method": "item/commandExecution/requestApproval", "id": 901, "params": {
                "threadId": msg["params"]["threadId"], "turnId": pending_turn,
                "itemId": "command-1", "reason": "should be denied"
            }})
            pending_server_request = "approval"
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
                        "candidate_citations": [],
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
