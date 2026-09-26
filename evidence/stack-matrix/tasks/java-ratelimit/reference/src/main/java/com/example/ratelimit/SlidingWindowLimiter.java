package com.example.ratelimit;

import java.time.Duration;
import java.util.ArrayDeque;
import java.util.HashMap;
import java.util.Map;

public final class SlidingWindowLimiter {
    private final int limit;
    private final long windowNanos;
    private final TimeSource time;
    private final Map<String, ArrayDeque<Long>> events = new HashMap<>();

    public SlidingWindowLimiter(int limit, Duration window, TimeSource time) {
        if (limit < 1 || window == null || window.isNegative() || window.isZero()) {
            throw new IllegalArgumentException("limit must be at least 1 and the window positive");
        }
        this.limit = limit;
        this.windowNanos = window.toNanos();
        this.time = time;
    }

    public synchronized boolean tryAcquire(String key) {
        long now = time.nanos();
        ArrayDeque<Long> log = expire(key, now);
        if (log.size() >= limit) {
            return false;
        }
        log.addLast(now);
        return true;
    }

    public synchronized int remaining(String key) {
        return limit - expire(key, time.nanos()).size();
    }

    private ArrayDeque<Long> expire(String key, long now) {
        ArrayDeque<Long> log = events.computeIfAbsent(key, k -> new ArrayDeque<>());
        while (!log.isEmpty() && log.peekFirst() <= now - windowNanos) {
            log.removeFirst();
        }
        return log;
    }
}
