import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useSpaceStatus } from './useSpaceStatus';
import { api } from '../services/api';
import { mockSpaceStatusIdle, mockSpaceStatusIndexing } from '../test/fixtures/search';

// Mock the API
vi.mock('../services/api', () => ({
  api: {
    spaces: {
      getStatus: vi.fn(),
      reindex: vi.fn(),
    },
  },
}));

const mockGetStatus = vi.mocked(api.spaces.getStatus);
const mockReindex = vi.mocked(api.spaces.reindex);

describe('useSpaceStatus', () => {
  const spaceKey = 'my-space';

  beforeEach(() => {
    mockGetStatus.mockClear();
    mockReindex.mockClear();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initial fetch', () => {
    it('fetches status on mount', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(mockGetStatus).toHaveBeenCalledWith(spaceKey);
      });
    });

    it('starts with loading state', () => {
      mockGetStatus.mockImplementation(() => new Promise(() => {}));

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      expect(result.current.isLoading).toBe(true);
    });

    it('returns status after successful fetch', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.status).toEqual(mockSpaceStatusIdle);
        expect(result.current.isLoading).toBe(false);
        expect(result.current.error).toBeNull();
      });
    });
  });

  describe('error handling', () => {
    it('sets error state on failure', async () => {
      mockGetStatus.mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.error).toBe('Network error');
      });
    });

    it('provides default error message for unknown errors', async () => {
      mockGetStatus.mockRejectedValue('Unknown');

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.error).toBe('Failed to fetch status');
      });
    });
  });

  describe('adaptive polling', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('polls every 60 seconds when idle', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      renderHook(() => useSpaceStatus(spaceKey));

      // Flush pending promises for initial fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      const initialCallCount = mockGetStatus.mock.calls.length;
      expect(initialCallCount).toBeGreaterThanOrEqual(1);

      // Advance 60 seconds (idle interval)
      await act(async () => {
        await vi.advanceTimersByTimeAsync(60000);
      });

      // Should have at least one more call
      expect(mockGetStatus.mock.calls.length).toBeGreaterThan(initialCallCount);
    });

    it('polls every 5 seconds when indexing', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIndexing);

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      // Flush pending promises for initial fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      // Verify state without waitFor (since we're using fake timers)
      expect(result.current.status?.indexing_in_progress).toBe(true);

      const initialCallCount = mockGetStatus.mock.calls.length;

      // Advance 5 seconds (active interval)
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5000);
      });

      // Should have at least one more call
      expect(mockGetStatus.mock.calls.length).toBeGreaterThan(initialCallCount);
    });

    it('stops polling on unmount', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      const { unmount } = renderHook(() => useSpaceStatus(spaceKey));

      // Flush pending promises for initial fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      const callCountBeforeUnmount = mockGetStatus.mock.calls.length;

      unmount();

      // Advance time - should not trigger more calls
      await act(async () => {
        await vi.advanceTimersByTimeAsync(120000);
      });

      // Call count should not increase after unmount
      expect(mockGetStatus.mock.calls.length).toBe(callCountBeforeUnmount);
    });
  });

  describe('refresh', () => {
    it('manually refreshes status', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      const callCountBeforeRefresh = mockGetStatus.mock.calls.length;

      // Manual refresh
      await act(async () => {
        await result.current.refresh();
      });

      // Should have at least one more call after refresh
      expect(mockGetStatus.mock.calls.length).toBeGreaterThan(callCountBeforeRefresh);
    });

    it('sets loading state during refresh', async () => {
      let resolvePromise: (value: typeof mockSpaceStatusIdle) => void;
      mockGetStatus.mockImplementation(
        () =>
          new Promise((resolve) => {
            resolvePromise = resolve;
          })
      );

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      // Initial fetch completes
      await act(async () => {
        resolvePromise!(mockSpaceStatusIdle);
      });

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      // Trigger refresh
      act(() => {
        result.current.refresh();
      });

      expect(result.current.isLoading).toBe(true);
    });
  });

  describe('reindex', () => {
    it('triggers reindex API call', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);
      mockReindex.mockResolvedValue({ status: 'reindexing' });

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.reindex();
      });

      expect(mockReindex).toHaveBeenCalledWith(spaceKey);
    });

    it('sets isReindexing during operation', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      let resolveReindex: (value: { status: string }) => void;
      mockReindex.mockImplementation(
        () =>
          new Promise((resolve) => {
            resolveReindex = resolve;
          })
      );

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      // Start reindex
      let reindexPromise: Promise<void>;
      act(() => {
        reindexPromise = result.current.reindex();
      });

      expect(result.current.isReindexing).toBe(true);

      // Resolve reindex
      await act(async () => {
        resolveReindex!({ status: 'reindexing' });
        await reindexPromise;
      });

      await waitFor(() => {
        expect(result.current.isReindexing).toBe(false);
      });
    });

    it('refreshes status after reindex', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);
      mockReindex.mockResolvedValue({ status: 'reindexing' });

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      const callCountBeforeReindex = mockGetStatus.mock.calls.length;

      await act(async () => {
        await result.current.reindex();
      });

      // Should have at least one more call after reindex
      expect(mockGetStatus.mock.calls.length).toBeGreaterThan(callCountBeforeReindex);
    });

    it('sets error on reindex failure', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);
      mockReindex.mockRejectedValue(new Error('Reindex failed'));

      const { result } = renderHook(() => useSpaceStatus(spaceKey));

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      await act(async () => {
        await result.current.reindex();
      });

      expect(result.current.error).toBe('Reindex failed');
    });
  });

  describe('space key changes', () => {
    it('refetches when spaceKey changes', async () => {
      mockGetStatus.mockResolvedValue(mockSpaceStatusIdle);

      const { rerender } = renderHook(({ key }) => useSpaceStatus(key), {
        initialProps: { key: 'space-1' },
      });

      await waitFor(() => {
        expect(mockGetStatus).toHaveBeenCalledWith('space-1');
      });

      rerender({ key: 'space-2' });

      await waitFor(() => {
        expect(mockGetStatus).toHaveBeenCalledWith('space-2');
      });
    });
  });
});
