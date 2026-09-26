package com.example.ratelimit;

import static org.junit.jupiter.api.Assertions.*;

import java.time.Duration;
import org.junit.jupiter.api.Test;

class TokenBucketTest {
    private final FakeTime time = new FakeTime();

    @Test
    void startsFullAndEmpties() {
        TokenBucket bucket = new TokenBucket(3, 1, Duration.ofSeconds(1), time);
        assertEquals(3, bucket.available());
        assertTrue(bucket.tryAcquire());
        assertTrue(bucket.tryAcquire());
        assertTrue(bucket.tryAcquire());
        assertFalse(bucket.tryAcquire());
        assertEquals(0, bucket.available());
    }

    @Test
    void refillsByWholePeriodsUpToCapacity() {
        TokenBucket bucket = new TokenBucket(5, 2, Duration.ofSeconds(1), time);
        assertTrue(bucket.tryAcquire(5));
        time.advance(Duration.ofMillis(999));
        assertEquals(0, bucket.available());
        time.advance(Duration.ofMillis(1));
        assertEquals(2, bucket.available());
        time.advance(Duration.ofSeconds(10));
        assertEquals(5, bucket.available());
    }

    @Test
    void partialPeriodsAreNotLost() {
        TokenBucket bucket = new TokenBucket(10, 1, Duration.ofSeconds(1), time);
        assertTrue(bucket.tryAcquire(10));
        time.advance(Duration.ofMillis(600));
        assertFalse(bucket.tryAcquire());
        time.advance(Duration.ofMillis(600));
        assertTrue(bucket.tryAcquire());
        time.advance(Duration.ofMillis(800));
        assertTrue(bucket.tryAcquire());
    }

    @Test
    void takesAllOrNothing() {
        TokenBucket bucket = new TokenBucket(4, 1, Duration.ofSeconds(1), time);
        assertTrue(bucket.tryAcquire(3));
        assertFalse(bucket.tryAcquire(2));
        assertEquals(1, bucket.available());
    }

    @Test
    void rejectsBadArguments() {
        assertThrows(IllegalArgumentException.class, () -> new TokenBucket(0, 1, Duration.ofSeconds(1), time));
        assertThrows(IllegalArgumentException.class, () -> new TokenBucket(1, 0, Duration.ofSeconds(1), time));
        assertThrows(IllegalArgumentException.class, () -> new TokenBucket(1, 1, Duration.ZERO, time));
        TokenBucket bucket = new TokenBucket(2, 1, Duration.ofSeconds(1), time);
        assertThrows(IllegalArgumentException.class, () -> bucket.tryAcquire(0));
        assertThrows(IllegalArgumentException.class, () -> bucket.tryAcquire(3));
    }
}
