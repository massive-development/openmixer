/*
  OpenMixer - lock-free single-producer / single-consumer ring buffer.
  Original implementation (AGPLv3). Capacity is rounded up to a power of two.
  Exactly one thread may call the write-side methods and exactly one thread
  the read-side methods. Neither side allocates, locks or blocks.
*/
#pragma once

#include <atomic>
#include <cstddef>
#include <cstring>
#include <memory>
#include <type_traits>

namespace openmixer
{

template <typename T>
class SpscRingBuffer
{
    static_assert (std::is_trivially_copyable<T>::value, "SpscRingBuffer requires trivially copyable T");

public:
    explicit SpscRingBuffer (size_t minimumCapacity)
    {
        size_t cap = 2;
        while (cap < minimumCapacity)
            cap <<= 1;
        capacity = cap;
        mask = cap - 1;
        storage.reset (new T[cap]());
    }

    size_t getCapacity() const noexcept { return capacity; }

    // Producer side ---------------------------------------------------------
    size_t getWritableCount() const noexcept
    {
        const auto w = writePos.load (std::memory_order_relaxed);
        const auto r = readPos.load (std::memory_order_acquire);
        return capacity - (w - r);
    }

    /** Writes up to n items; returns the number actually written. */
    size_t write (const T* src, size_t n) noexcept
    {
        const auto w = writePos.load (std::memory_order_relaxed);
        const auto r = readPos.load (std::memory_order_acquire);
        const size_t room = capacity - (w - r);
        const size_t toWrite = room < n ? room : n;
        copyIn (src, w & mask, toWrite);
        writePos.store (w + toWrite, std::memory_order_release);
        return toWrite;
    }

    // Consumer side ---------------------------------------------------------
    size_t getReadableCount() const noexcept
    {
        const auto r = readPos.load (std::memory_order_relaxed);
        const auto w = writePos.load (std::memory_order_acquire);
        return w - r;
    }

    /** Reads up to n items; returns the number actually read. */
    size_t read (T* dst, size_t n) noexcept
    {
        const auto r = readPos.load (std::memory_order_relaxed);
        const auto w = writePos.load (std::memory_order_acquire);
        const size_t avail = w - r;
        const size_t toRead = avail < n ? avail : n;
        copyOut (dst, r & mask, toRead);
        readPos.store (r + toRead, std::memory_order_release);
        return toRead;
    }

    /** Discards up to n items (consumer side); returns the number discarded. */
    size_t skip (size_t n) noexcept
    {
        const auto r = readPos.load (std::memory_order_relaxed);
        const auto w = writePos.load (std::memory_order_acquire);
        const size_t avail = w - r;
        const size_t toSkip = avail < n ? avail : n;
        readPos.store (r + toSkip, std::memory_order_release);
        return toSkip;
    }

private:
    void copyIn (const T* src, size_t start, size_t n) noexcept
    {
        const size_t first = (capacity - start) < n ? (capacity - start) : n;
        if (first > 0) std::memcpy (storage.get() + start, src, first * sizeof (T));
        if (n > first) std::memcpy (storage.get(), src + first, (n - first) * sizeof (T));
    }

    void copyOut (T* dst, size_t start, size_t n) const noexcept
    {
        const size_t first = (capacity - start) < n ? (capacity - start) : n;
        if (first > 0) std::memcpy (dst, storage.get() + start, first * sizeof (T));
        if (n > first) std::memcpy (dst + first, storage.get(), (n - first) * sizeof (T));
    }

    size_t capacity = 0, mask = 0;
    std::unique_ptr<T[]> storage;
    alignas (64) std::atomic<size_t> writePos { 0 };
    alignas (64) std::atomic<size_t> readPos { 0 };
};

} // namespace openmixer
