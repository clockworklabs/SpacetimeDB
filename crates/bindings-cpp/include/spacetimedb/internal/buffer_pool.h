#pragma once

#include <vector>
#include <cstdint>

namespace SpacetimeDB {
namespace Internal {

/**
 * @brief Thread-local buffer pool for reducing allocations
 * 
 * This implements the same strategy as Rust's IterBuf:
 * - Maintains a pool of reusable buffers
 * - Buffers are returned to the pool when no longer needed
 * - After warmup, typical operations have zero allocations
 * 
 * The pool is thread-local (though WASM is single-threaded anyway).
 * Default buffer size is 64 KiB, matching Rust's DEFAULT_BUFFER_CAPACITY.
 */

// Match Rust: ROW_ITER_CHUNK_SIZE * 2 = 32*1024 * 2 = 64 KiB
constexpr size_t DEFAULT_BUFFER_CAPACITY = 64 * 1024;

/**
 * @brief Get the thread-local buffer pool
 * 
 * Implemented as a function with a static local to ensure
 * initialization before first use.
 */
inline std::vector<std::vector<uint8_t>>& get_buffer_pool() {
    thread_local std::vector<std::vector<uint8_t>> pool;
    return pool;
}

/**
 * @brief RAII wrapper for pooled buffers
 * 
 * Usage:
 * ```cpp
 * // Temporary buffer - returns to pool on destruction
 * {
 *     IterBuf buf = IterBuf::take();
 *     buf.reserve(1024);
 *     // ... use buffer ...
 * }  // Buffer automatically returned to pool
 * 
 * // Transfer ownership - buffer NOT returned to pool
 * std::vector<uint8_t> owned = IterBuf::take().release();
 * ```
 */
class IterBuf {
    std::vector<uint8_t> buffer_;
    bool released_;  // Track if ownership was transferred
    
public:
    /**
     * @brief Take a buffer from the pool, or allocate a new one
     * 
     * After warmup, this will typically reuse a pooled buffer
     * with 64 KiB pre-allocated capacity.
     */
    static IterBuf take() {
        auto& pool = get_buffer_pool();
        if (!pool.empty()) {
            auto buf = std::move(pool.back());
            pool.pop_back();
            return IterBuf(std::move(buf));
        }
        
        // First time or pool exhausted - allocate new buffer
        std::vector<uint8_t> buf;
        buf.reserve(DEFAULT_BUFFER_CAPACITY);
        return IterBuf(std::move(buf));
    }
    
    /**
     * @brief Destructor - return buffer to pool if not released
     */
    ~IterBuf() {
        if (!released_) {
            buffer_.clear();  // Clear contents but keep capacity
            get_buffer_pool().push_back(std::move(buffer_));
        }
    }
    
    // No copy - only move
    IterBuf(const IterBuf&) = delete;
    IterBuf& operator=(const IterBuf&) = delete;
    
    // Move constructor and assignment
    IterBuf(IterBuf&& other) noexcept 
        : buffer_(std::move(other.buffer_))
        , released_(other.released_) {
        other.released_ = true;  // Prevent other from returning to pool
    }
    
    IterBuf& operator=(IterBuf&& other) noexcept {
        if (this != &other) {
            // Return our current buffer to pool if we have one
            if (!released_) {
                buffer_.clear();
                get_buffer_pool().push_back(std::move(buffer_));
            }
            
            buffer_ = std::move(other.buffer_);
            released_ = other.released_;
            other.released_ = true;
        }
        return *this;
    }
    
    /**
     * @brief Get mutable reference to the buffer
     * 
     * Use this for operations that need to modify the buffer
     * while keeping it in the pool-managed scope.
     */
    std::vector<uint8_t>& get() {
        return buffer_;
    }
    
    /**
     * @brief Get const reference to the buffer
     */
    const std::vector<uint8_t>& get() const {
        return buffer_;
    }
    
    /**
     * @brief Release ownership of the buffer
     * 
     * The buffer will NOT be returned to the pool when this IterBuf
     * is destroyed. Use this when you need to transfer ownership.
     */
    std::vector<uint8_t> release() {
        released_ = true;
        return std::move(buffer_);
    }
    
    // Convenience methods for common operations
    
    void clear() {
        buffer_.clear();
    }
    
    void reserve(size_t capacity) {
        buffer_.reserve(capacity);
    }
    
    size_t size() const {
        return buffer_.size();
    }
    
    size_t capacity() const {
        return buffer_.capacity();
    }
    
    void resize(size_t size) {
        buffer_.resize(size);
    }
    
    uint8_t* data() {
        return buffer_.data();
    }
    
    const uint8_t* data() const {
        return buffer_.data();
    }
    
    // Iterator support
    auto begin() { return buffer_.begin(); }
    auto end() { return buffer_.end(); }
    auto begin() const { return buffer_.begin(); }
    auto end() const { return buffer_.end(); }
    
private:
    explicit IterBuf(std::vector<uint8_t>&& buf) 
        : buffer_(std::move(buf))
        , released_(false) {
    }
};

} // namespace Internal
} // namespace SpacetimeDB
