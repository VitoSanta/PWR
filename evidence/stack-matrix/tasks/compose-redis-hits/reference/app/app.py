import os
import re

import redis
from flask import Flask, jsonify, request

app = Flask(__name__)
store = redis.Redis.from_url(os.environ.get("REDIS_URL", "redis://redis:6379/0"),
                             socket_timeout=2, socket_connect_timeout=2, decode_responses=True)
PAGE = re.compile(r"^[a-z0-9-]{1,64}$")
LIMIT, WINDOW = 5, 60


def error(status, message, **extra):
    return jsonify({"error": message, **extra}), status


@app.get("/health")
def health():
    try:
        store.ping()
        return jsonify({"status": "ok", "redis": "ok"})
    except redis.RedisError:
        return jsonify({"status": "degraded", "redis": "down"}), 503


@app.post("/hit/<page>")
def hit(page):
    if not PAGE.match(page):
        return error(400, "page must be 1-64 of [a-z0-9-]")
    client = request.headers.get("X-Client-Id", "").strip()
    if not client:
        return error(400, "X-Client-Id is required")
    key = f"rl:{client}"
    used = store.incr(key)
    if used == 1:
        store.expire(key, WINDOW)
    if used > LIMIT:
        left = store.ttl(key)
        return error(429, "rate limited", retry_after=max(1, left if left and left > 0 else 1))
    count = int(store.zincrby("hits", 1, page))
    return jsonify({"page": page, "count": count})


@app.get("/count/<page>")
def count(page):
    if not PAGE.match(page):
        return error(400, "page must be 1-64 of [a-z0-9-]")
    return jsonify({"page": page, "count": int(store.zscore("hits", page) or 0)})


@app.get("/top")
def top():
    raw = request.args.get("n", "3")
    if not raw.isdigit() or not 1 <= int(raw) <= 50:
        return error(400, "n must be 1-50")
    pages = sorted(store.zrange("hits", 0, -1, withscores=True), key=lambda item: (-item[1], item[0]))
    return jsonify([{"page": page, "count": int(score)} for page, score in pages[: int(raw)]])
