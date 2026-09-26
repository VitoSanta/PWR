package com.example.ratelimit;

/** Where the limiters read the time; tests pass a fake one. */
public interface TimeSource {
    long nanos();

    TimeSource SYSTEM = System::nanoTime;
}
