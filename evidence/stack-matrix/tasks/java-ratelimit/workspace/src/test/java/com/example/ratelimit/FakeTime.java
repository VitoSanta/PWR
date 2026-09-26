package com.example.ratelimit;

import java.time.Duration;

final class FakeTime implements TimeSource {
    private long now = 1_000_000_000L;

    @Override
    public long nanos() {
        return now;
    }

    void advance(Duration by) {
        now += by.toNanos();
    }
}
