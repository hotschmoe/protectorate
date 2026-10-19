package sidecar

import (
	"sync"
	"time"
)

// CachedValue provides thread-safe caching with TTL
type CachedValue[T any] struct {
	ttl      time.Duration
	fetch    func() T
	mu       sync.RWMutex
	value    T
	hasValue bool
	cachedAt time.Time
}

// NewCachedValue creates a cached value with the given TTL and fetch function
func NewCachedValue[T any](ttl time.Duration, fetch func() T) *CachedValue[T] {
	return &CachedValue[T]{
		ttl:   ttl,
		fetch: fetch,
	}
}

// Get returns the cached value, refreshing if stale
func (c *CachedValue[T]) Get() T {
	c.mu.RLock()
	if c.hasValue && time.Since(c.cachedAt) < c.ttl {
		value := c.value
		c.mu.RUnlock()
		return value
	}
	c.mu.RUnlock()

	c.mu.Lock()
	defer c.mu.Unlock()

	if c.hasValue && time.Since(c.cachedAt) < c.ttl {
		return c.value
	}

	c.value = c.fetch()
	c.hasValue = true
	c.cachedAt = time.Now()
	return c.value
}
