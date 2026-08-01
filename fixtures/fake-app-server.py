import json
import os
import sys

pending_turn = None
pending_server_request = None
turn_counter = 0
thread_counter = 0
active_dynamic_tools = []
thread_dynamic_tools = {}
last_thread_request = None
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
        }, {
            "id": "gpt-sol-id", "model": "gpt-5.6-sol", "displayName": "GPT-5.6-Sol",
            "description": "paper analysis primary", "hidden": False, "isDefault": False,
            "defaultReasoningEffort": "high",
            "supportedReasoningEfforts": [
                {"reasoningEffort": "medium", "description": "balanced"},
                {"reasoningEffort": "high", "description": "deep"}
            ],
            "serviceTiers": [{"id": "priority", "name": "Fast", "description": "fast"}]
        }, {
            "id": "gpt-terra-id", "model": "gpt-5.6-terra", "displayName": "GPT-5.6-Terra",
            "description": "paper analysis fallback", "hidden": False, "isDefault": False,
            "defaultReasoningEffort": "medium",
            "supportedReasoningEfforts": [
                {"reasoningEffort": "low", "description": "fast"},
                {"reasoningEffort": "medium", "description": "balanced"}
            ],
            "serviceTiers": []
        }, {
            "id": "gpt-luna-id", "model": "gpt-5.6-luna", "displayName": "GPT-5.6-Luna",
            "description": "paper analysis fallback", "hidden": False, "isDefault": False,
            "defaultReasoningEffort": "low",
            "supportedReasoningEfforts": [
                {"reasoningEffort": "low", "description": "fast"},
                {"reasoningEffort": "medium", "description": "balanced"}
            ],
            "serviceTiers": []
        }]}})
    elif method == "skills/list":
        cwd = msg["params"]["cwds"][0]
        send({"id": msg["id"], "result": {"data": [{
            "cwd": cwd,
            "skills": [{
                "name": "paper-research",
                "description": "Read, compare, and synthesize papers",
                "enabled": True,
                "path": f"{cwd}/.codex/skills/paper-research/SKILL.md",
                "scope": "repo",
                "interface": {
                    "displayName": "Paper Research",
                    "shortDescription": "Evidence-first paper research"
                },
                "dependencies": {"tools": []}
            }],
            "errors": []
        }]}})
    elif method == "mcpServerStatus/list":
        send({"id": msg["id"], "result": {"data": [{
            "name": "openalex",
            "authStatus": "oAuth",
            "serverInfo": {
                "name": "openalex-server",
                "version": "1.0.0",
                "title": "OpenAlex",
                "description": "Search scholarly metadata"
            },
            "tools": {
                "works/search": {
                    "name": "works/search",
                    "title": "Search works",
                    "description": "Search scholarly works",
                    "inputSchema": {"type": "object"}
                }
            },
            "resources": [],
            "resourceTemplates": []
        }], "nextCursor": None}})
    elif method == "thread/start":
        if reject_dynamic_tools and "dynamicTools" in msg["params"]:
            send({"id": msg["id"], "error": {"code": -32602, "message": "dynamicTools unsupported"}})
            continue
        thread_counter += 1
        thread_id = "thread-fake" if thread_counter == 1 else f"thread-fake-{thread_counter}"
        active_dynamic_tools = msg["params"].get("dynamicTools", [])
        thread_dynamic_tools[thread_id] = active_dynamic_tools
        last_thread_request = {"method": method, "params": msg["params"]}
        send({"id": msg["id"], "result": {"thread": {"id": thread_id}}})
    elif method == "thread/resume":
        if reject_dynamic_tools and "dynamicTools" in msg["params"]:
            send({"id": msg["id"], "error": {"code": -32602, "message": "dynamicTools unsupported"}})
            continue
        active_dynamic_tools = thread_dynamic_tools.get(msg["params"]["threadId"], [])
        last_thread_request = {"method": method, "params": msg["params"]}
        send({"id": msg["id"], "result": {"thread": {"id": msg["params"]["threadId"]}}})
    elif method in ["thread/archive", "thread/unarchive", "thread/delete"]:
        result = {"thread": {"id": msg["params"]["threadId"]}} if method == "thread/unarchive" else {}
        send({"id": msg["id"], "result": result})
    elif method == "turn/start":
        turn_counter += 1
        pending_turn = f"turn-fake-{turn_counter}"
        send({"id": msg["id"], "result": {"turn": {"id": pending_turn}}})
        text = msg["params"]["input"][0]["text"]
        if last_thread_request is not None and "observe-thread-params" in text:
            send({"method": "test/thread-params", "params": last_thread_request})
        if "runtime-tmp" in text:
            send({"method": "test/runtime-tmp", "params": {"path": os.environ.get("TMPDIR")}})
        if "settings" in text or "skill-turn" in text:
            send({"method": "test/turn-params", "params": msg["params"]})
        if "fail-me" in text:
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "failed", "error": {
                "message": "structured output rejected",
                "additionalDetails": "schema mismatch",
                "codexErrorInfo": "ResponseSerializationFailure",
                "httpStatusCode": 422
            }}}})
            pending_turn = None
        elif "capacity-me" in text or ("capacity-sol" in text and msg["params"]["model"] == "gpt-5.6-sol"):
            send({"method": "turn/completed", "params": {"threadId": msg["params"]["threadId"], "turn": {"id": pending_turn, "items": [], "status": "failed", "error": {
                "message": "Selected model is at capacity. Please try a different model.",
                "codexErrorInfo": "ServerOverloaded",
                "httpStatusCode": 503
            }}}})
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
