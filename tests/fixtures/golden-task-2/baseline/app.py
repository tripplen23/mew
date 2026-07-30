"""Task manager API — Flask baseline for golden task 2.

Framework migration target: Flask → Rust Axum.
Public contract: OpenAPI 3.0 spec in openapi.yaml.
"""
from flask import Flask, request, jsonify

app = Flask(__name__)

tasks: dict[int, dict] = {}
_next_id = 1


@app.route("/tasks", methods=["GET"])
def list_tasks():
    """List all tasks. Returns empty array when no tasks exist."""
    return jsonify(list(tasks.values()))


@app.route("/tasks", methods=["POST"])
def create_task():
    """Create a new task. Requires JSON body with 'title' field."""
    global _next_id
    body = request.get_json(force=True)
    if not body or "title" not in body:
        return jsonify({"error": "title is required"}), 400
    task = {"id": _next_id, "title": body["title"], "done": False}
    tasks[_next_id] = task
    _next_id += 1
    return jsonify(task), 201


@app.route("/tasks/<int:task_id>", methods=["GET"])
def get_task(task_id: int):
    """Get a single task by ID. Returns 404 if not found."""
    task = tasks.get(task_id)
    if task is None:
        return jsonify({"error": "not found"}), 404
    return jsonify(task)


@app.route("/tasks/<int:task_id>", methods=["DELETE"])
def delete_task(task_id: int):
    """Delete a task by ID. Returns 204 on success, 404 if not found."""
    if task_id not in tasks:
        return jsonify({"error": "not found"}), 404
    del tasks[task_id]
    return "", 204


@app.route("/health", methods=["GET"])
def health():
    """Health check endpoint — always returns 200."""
    return jsonify({"status": "ok"})
