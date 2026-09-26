package com.example.ratelimit;

import static org.junit.jupiter.api.Assertions.*;

import java.time.Duration;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;

class HiddenConcurrencyTest {
    @Test
    void aBucketNeverHandsOutMoreThanItHas() throws Exception {
        FakeTime time = new FakeTime();
        TokenBucket bucket = new TokenBucket(100, 1, Duration.ofHours(1), time);
        AtomicInteger granted = new AtomicInteger();
        runConcurrently(() -> { if (bucket.tryAcquire()) granted.incrementAndGet(); });
        assertEquals(100, granted.get());
        assertEquals(0, bucket.available());
    }

    @Test
    void aWindowNeverAdmitsMoreThanItsLimit() throws Exception {
        FakeTime time = new FakeTime();
        SlidingWindowLimiter limiter = new SlidingWindowLimiter(50, Duration.ofMinutes(1), time);
        AtomicInteger granted = new AtomicInteger();
        runConcurrently(() -> { if (limiter.tryAcquire("shared")) granted.incrementAndGet(); });
        assertEquals(50, granted.get());
    }

    @Test
    void theSystemTimeSourceIsMonotonic() {
        long first = TimeSource.SYSTEM.nanos();
        assertTrue(TimeSource.SYSTEM.nanos() >= first);
    }

    @Test
    void manyPeriodsRefillOnlyToCapacityAndKeepTheRemainder() {
        FakeTime time = new FakeTime();
        TokenBucket bucket = new TokenBucket(3, 1, Duration.ofMillis(100), time);
        assertTrue(bucket.tryAcquire(3));
        time.advance(Duration.ofMillis(250));
        assertEquals(2, bucket.available());
        assertTrue(bucket.tryAcquire(2));
        time.advance(Duration.ofMillis(50));
        assertEquals(1, bucket.available());
    }

    private static void runConcurrently(Runnable attempt) throws Exception {
        ExecutorService pool = Executors.newFixedThreadPool(8);
        CountDownLatch start = new CountDownLatch(1);
        for (int thread = 0; thread < 8; thread++) {
            pool.submit(() -> {
                start.await();
                for (int i = 0; i < 1000; i++) attempt.run();
                return null;
            });
        }
        start.countDown();
        pool.shutdown();
        assertTrue(pool.awaitTermination(30, TimeUnit.SECONDS));
    }
}
