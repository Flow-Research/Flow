import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useDistributedSearch } from './useDistributedSearch';
import { api } from '../services/api';
import { mockSearchResponse, mockEmptySearchResponse } from '../test/fixtures/search';

// Mock the API
vi.mock('../services/api', () => ({
  api: {
    search: {
      distributed: vi.fn(),
    },
  },
  ApiError: class ApiError extends Error {
    status: number;
    constructor(message: string, status: number) {
      super(message);
      this.status = status;
    }
  },
}));

const mockDistributed = vi.mocked(api.search.distributed);

describe('useDistributedSearch', () => {
  beforeEach(() => {
    mockDistributed.mockClear();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initial state', () => {
    it('starts in idle state', () => {
      const { result } = renderHook(() => useDistributedSearch());

      expect(result.current.state).toBe('idle');
      expect(result.current.results).toEqual([]);
      expect(result.current.stats).toBeNull();
      expect(result.current.error).toBeNull();
      expect(result.current.hasMore).toBe(false);
    });
  });

  describe('search with debounce', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('sets loading state immediately', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'all');
      });

      expect(result.current.state).toBe('loading');
    });

    it('debounces search requests', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'all');
      });

      // Search should not be called yet (debounce)
      expect(mockDistributed).not.toHaveBeenCalled();

      // Advance past debounce time
      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });

      expect(mockDistributed).toHaveBeenCalledTimes(1);
    });

    it('returns results after successful search', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      // Use fake timer flush instead of waitFor
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.state).toBe('success');
      expect(result.current.results).toEqual(mockSearchResponse.results);
      expect(result.current.stats).toEqual({
        localCount: mockSearchResponse.local_count,
        networkCount: mockSearchResponse.network_count,
        peersQueried: mockSearchResponse.peers_queried,
        peersResponded: mockSearchResponse.peers_responded,
        totalFound: mockSearchResponse.total_found,
        elapsedMs: mockSearchResponse.elapsed_ms,
      });
    });

    it('clears results for empty query', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      // First search with results
      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.results).toHaveLength(2);

      // Clear with empty query
      act(() => {
        result.current.search('', 'all');
      });

      expect(result.current.state).toBe('idle');
      expect(result.current.results).toEqual([]);
      expect(result.current.stats).toBeNull();
    });

    it('trims whitespace from query', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('   ', 'all');
      });

      // Empty query (whitespace only) should clear
      expect(result.current.state).toBe('idle');
      expect(mockDistributed).not.toHaveBeenCalled();
    });

    it('clears pending debounced search', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'all');
      });

      // Clear before debounce completes
      act(() => {
        result.current.clear();
      });

      // Advance timer past debounce
      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });

      // Should not have called API
      expect(mockDistributed).not.toHaveBeenCalled();
    });
  });

  describe('error handling', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('sets error state on failure', async () => {
      mockDistributed.mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.state).toBe('error');
      // Hook wraps all non-ApiError errors with user-friendly message
      expect(result.current.error).toBe('Search failed. Please try again.');
    });

    it('provides default error message for unknown errors', async () => {
      mockDistributed.mockRejectedValue('Unknown');

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      // Hook wraps all errors with user-friendly message
      expect(result.current.error).toBe('Search failed. Please try again.');
    });
  });

  describe('pagination', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('calculates hasMore correctly', async () => {
      // Response where more results exist
      mockDistributed.mockResolvedValue({
        ...mockSearchResponse,
        total_found: 50,
      });

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.hasMore).toBe(true);
    });

    it('sets hasMore to false when all results loaded', async () => {
      mockDistributed.mockResolvedValue(mockEmptySearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.hasMore).toBe(false);
    });

    it('appends results on loadMore', async () => {
      // First page
      mockDistributed.mockResolvedValueOnce({
        ...mockSearchResponse,
        total_found: 50,
      });

      // Second page with different results
      const secondPageResults = [
        { ...mockSearchResponse.results[0], cid: 'bafynewresult1' },
        { ...mockSearchResponse.results[1], cid: 'bafynewresult2' },
      ];
      mockDistributed.mockResolvedValueOnce({
        ...mockSearchResponse,
        results: secondPageResults,
        total_found: 50,
      });

      const { result } = renderHook(() => useDistributedSearch());

      // Initial search
      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.results).toHaveLength(2);

      // Load more
      await act(async () => {
        result.current.loadMore();
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.results).toHaveLength(4);
    });

    it('does not loadMore when already loading', async () => {
      mockDistributed.mockImplementation(() => new Promise(() => {})); // Never resolves

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      // Still loading from search, try to load more
      act(() => {
        result.current.loadMore();
      });

      // Should only have been called once (from search)
      expect(mockDistributed).toHaveBeenCalledTimes(1);
    });
  });

  describe('clear', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('resets all state', async () => {
      mockDistributed.mockResolvedValue(mockSearchResponse);

      const { result } = renderHook(() => useDistributedSearch());

      await act(async () => {
        result.current.search('test', 'all');
        await vi.advanceTimersByTimeAsync(300);
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      expect(result.current.results).toHaveLength(2);

      act(() => {
        result.current.clear();
      });

      expect(result.current.state).toBe('idle');
      expect(result.current.results).toEqual([]);
      expect(result.current.stats).toBeNull();
      expect(result.current.error).toBeNull();
      expect(result.current.hasMore).toBe(false);
    });
  });
});
