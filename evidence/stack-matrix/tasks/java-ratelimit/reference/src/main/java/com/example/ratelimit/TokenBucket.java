package com.example.ratelimit;

import java.time.Duration;

public final class TokenBucket {
    private final long capacity;
    private final long refillTokens;
    private final long periodNanos;
    private final TimeSource time;
    private long tokens;
    private long lastRefill;

    public TokenBucket(long capacity, long refillTokens, Duration refillPeriod, TimeSource time) {
        if (capacity < 1 || refillTokens < 1 || refillPeriod == null || refillPeriod.isNegative() || refillPeriod.isZero()) {
            throw new IllegalArgumentException("capacity and refill must be at least 1 and the period positive");
        }
        this.capacity = capacity;
        this.refillTokens = refillTokens;
        this.periodNanos = refillPeriod.toNanos();
        this.time = time;
        this.tokens = capacity;
        this.lastRefill = time.nanos();
    }

    public boolean tryAcquire() {
        return tryAcquire(1);
    }

    public synchronized boolean tryAcquire(long wanted) {
        if (wanted < 1 || wanted > capacity) {
            throw new IllegalArgumentException("tokens must be between 1 and the capacity");
        }
        refill();
        if (tokens < wanted) {
            return false;
        }
        tokens -= wanted;
        return true;
    }

    public synchronized long available() {
        refill();
        return tokens;
    }

    private void refill() {
        long now = time.nanos();
        long periods = (now - lastRefill) / periodNanos;
        if (periods <= 0) {
            return;
        }
        lastRefill += periods * periodNanos;
        long added = periods > capacity ? capacity : periods * refillTokens;
        tokens = Math.min(capacity, tokens + added);
    }
}
