package com.example.ratelimit;

import static org.junit.jupiter.api.Assertions.*;

import java.time.Duration;
import org.junit.jupiter.api.Test;

class SlidingWindowLimiterTest {
    private final FakeTime time = new FakeTime();

    @Test
    void limitsEachKeySeparately() {
        SlidingWindowLimiter limiter = new SlidingWindowLimiter(2, Duration.ofMinutes(1), time);
        assertTrue(limiter.tryAcquire("alice"));
        assertTrue(limiter.tryAcquire("alice"));
        assertFalse(limiter.tryAcquire("alice"));
        assertTrue(limiter.tryAcquire("bob"));
        assertEquals(0, limiter.remaining("alice"));
        assertEquals(1, limiter.remaining("bob"));
        assertEquals(2, limiter.remaining("carol"));
    }

    @Test
    void anAcquisitionExactlyAWindowAgoNoLongerCounts() {
        SlidingWindowLimiter limiter = new SlidingWindowLimiter(1, Duration.ofSeconds(10), time);
        assertTrue(limiter.tryAcquire("k"));
        time.advance(Duration.ofMillis(9_999));
        assertFalse(limiter.tryAcquire("k"));
        time.advance(Duration.ofMillis(1));
        assertTrue(limiter.tryAcquire("k"));
    }

    @Test
    void theWindowSlides() {
        SlidingWindowLimiter limiter = new SlidingWindowLimiter(3, Duration.ofSeconds(10), time);
        assertTrue(limiter.tryAcquire("k"));
        time.advance(Duration.ofSeconds(4));
        assertTrue(limiter.tryAcquire("k"));
        assertTrue(limiter.tryAcquire("k"));
        assertFalse(limiter.tryAcquire("k"));
        time.advance(Duration.ofSeconds(6));
        assertEquals(1, limiter.remaining("k"));
        assertTrue(limiter.tryAcquire("k"));
        assertFalse(limiter.tryAcquire("k"));
    }

    @Test
    void aRefusalRecordsNothing() {
        SlidingWindowLimiter limiter = new SlidingWindowLimiter(1, Duration.ofSeconds(10), time);
        assertTrue(limiter.tryAcquire("k"));
        time.advance(Duration.ofSeconds(5));
        assertFalse(limiter.tryAcquire("k"));
        time.advance(Duration.ofSeconds(5));
        assertTrue(limiter.tryAcquire("k"));
    }

    @Test
    void rejectsBadArguments() {
        assertThrows(IllegalArgumentException.class, () -> new SlidingWindowLimiter(0, Duration.ofSeconds(1), time));
        assertThrows(IllegalArgumentException.class, () -> new SlidingWindowLimiter(1, Duration.ofSeconds(-1), time));
    }
}
