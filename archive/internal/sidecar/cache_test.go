package sidecar

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestCachedValue_Get_FreshValue(t *testing.T) {
	callCount := 0
	cache := NewCachedValue(time.Hour, func() string {
		callCount++
		return "test-value"
	})

	result := cache.Get()

	if result != "test-value" {
		t.Errorf("expected 'test-value', got %q", result)
	}
	if callCount != 1 {
		t.Errorf("expected fetch to be called once, was called %d times", callCount)
	}
}

func TestCachedValue_Get_ReturnsCached(t *testing.T) {
	callCount := 0
	cache := NewCachedValue(time.Hour, func() string {
		callCount++
		return "test-value"
	})

	// First call
	cache.Get()
	// Second call should use cache
	result := cache.Get()

	if result != "test-value" {
		t.Errorf("expected 'test-value', got %q", result)
	}
	if callCount != 1 {
		t.Errorf("expected fetch to be called once, was called %d times", callCount)
	}
}

func TestCachedValue_Get_RefreshesAfterTTL(t *testing.T) {
	callCount := 0
	ttl := 50 * time.Millisecond
	cache := NewCachedValue(ttl, func() int {
		callCount++
		return callCount
	})

	// First call
	result1 := cache.Get()
	if result1 != 1 {
		t.Errorf("expected 1, got %d", result1)
	}

	// Wait for TTL to expire
	time.Sleep(ttl + 10*time.Millisecond)

	// Second call should refresh
	result2 := cache.Get()
	if result2 != 2 {
		t.Errorf("expected 2 after TTL expiry, got %d", result2)
	}
	if callCount != 2 {
		t.Errorf("expected fetch to be called twice, was called %d times", callCount)
	}
}

func TestCachedValue_Get_ConcurrentAccess(t *testing.T) {
	var fetchCount int64
	cache := NewCachedValue(time.Hour, func() int {
		atomic.AddInt64(&fetchCount, 1)
		time.Sleep(10 * time.Millisecond) // Simulate work
		return 42
	})

	var wg sync.WaitGroup
	results := make([]int, 10)

	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			results[idx] = cache.Get()
		}(i)
	}

	wg.Wait()

	// All results should be the same
	for i, r := range results {
		if r != 42 {
			t.Errorf("result[%d] = %d, expected 42", i, r)
		}
	}

	// Fetch should only be called a small number of times due to locking
	if fetchCount > 3 {
		t.Errorf("expected fetch to be called at most 3 times due to locking, was called %d times", fetchCount)
	}
}
