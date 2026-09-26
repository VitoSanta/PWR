# ratelimit

Two rate limiters for the API gateway, in plain Java 21 (no dependencies
besides JUnit for the tests). Package `com.example.ratelimit`.

```java
/** Where the limiters read the time; tests pass a fake one. */
public interface TimeSource {
    long nanos();                 // monotonic nanoseconds
    TimeSource SYSTEM = System::nanoTime;
}

public final class TokenBucket {
    public TokenBucket(long capacity, long refillTokens, Duration refillPeriod, TimeSource time);
    public boolean tryAcquire();              // one token
    public boolean tryAcquire(long tokens);
    public long available();
}

public final class SlidingWindowLimiter {
    public SlidingWindowLimiter(int limit, Duration window, TimeSource time);
    public boolean tryAcquire(String key);
    public int remaining(String key);
}
```

## TokenBucket

- Starts full (`capacity` tokens).
- Every whole `refillPeriod` that passes adds `refillTokens`, never above
  `capacity`. Time in a period not yet complete is not lost: it counts toward
  the next refill.
- `tryAcquire(n)` takes `n` tokens if they are all there and returns `true`;
  otherwise takes none and returns `false`.
- The constructor throws `IllegalArgumentException` for a capacity or refill
  amount below 1 or a period that is not positive; `tryAcquire(n)` throws it
  for `n < 1` or `n > capacity`.
- Safe to use from several threads at once.

## SlidingWindowLimiter

- Each key is limited separately: at most `limit` acquisitions in any window
  of length `window`. The window is `(now - window, now]`: an acquisition
  made exactly `window` ago no longer counts.
- `tryAcquire(key)` records an acquisition and returns `true` if the key is
  under its limit; otherwise records nothing and returns `false`.
- `remaining(key)` is how many acquisitions the key could make now.
- The constructor throws `IllegalArgumentException` for a limit below 1 or a
  window that is not positive. Safe to use from several threads at once.

Run the tests with `mvn -q test`.
