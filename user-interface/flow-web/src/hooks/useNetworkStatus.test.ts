import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useNetworkStatus } from './useNetworkStatus';
import { api } from '../services/api';
import { mockNetworkStatus } from '../test/fixtures/search';

// Mock the API
vi.mock('../services/api', () => ({
  api: {
    network: {
      status: vi.fn(),
    },
  },
}));

const mockStatus = vi.mocked(api.network.status);

describe('useNetworkStatus', () => {
  beforeEach(() => {
    mockStatus.mockClear();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initial fetch', () => {
    it('fetches status on mount', async () => {
      mockStatus.mockResolvedValue(mockNetworkStatus);

      renderHook(() => useNetworkStatus());

      await waitFor(() => {
        expect(mockStatus).toHaveBeenCalledTimes(1);
      });
    });

    it('starts with loading state', () => {
      mockStatus.mockImplementation(() => new Promise(() => {}));

      const { result } = renderHook(() => useNetworkStatus());

      expect(result.current.isLoading).toBe(true);
    });

    it('returns connected state after successful fetch', async () => {
      mockStatus.mockResolvedValue(mockNetworkStatus);

      const { result } = renderHook(() => useNetworkStatus());

      await waitFor(() => {
        expect(result.current.isConnected).toBe(true);
        expect(result.current.peerCount).toBe(3);
        expect(result.current.peers).toEqual(mockNetworkStatus.peers);
        expect(result.current.isLoading).toBe(false);
        expect(result.current.error).toBeNull();
      });
    });
  });

  describe('error handling', () => {
    it('sets disconnected status on error', async () => {
      mockStatus.mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useNetworkStatus());

      await waitFor(() => {
        expect(result.current.isConnected).toBe(false);
        expect(result.current.peerCount).toBe(0);
        expect(result.current.peers).toEqual([]);
        expect(result.current.error).toBe('Network error');
      });
    });

    it('provides default error message for unknown errors', async () => {
      mockStatus.mockRejectedValue('Unknown');

      const { result } = renderHook(() => useNetworkStatus());

      await waitFor(() => {
        expect(result.current.error).toBe('Failed to fetch network status');
      });
    });
  });

  describe('polling', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('polls every 30 seconds', async () => {
      mockStatus.mockResolvedValue(mockNetworkStatus);

      renderHook(() => useNetworkStatus());

      // Flush pending promises for initial fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      const initialCallCount = mockStatus.mock.calls.length;
      expect(initialCallCount).toBeGreaterThanOrEqual(1);

      // Advance 30 seconds
      await act(async () => {
        await vi.advanceTimersByTimeAsync(30000);
      });

      // Should have at least one more call
      expect(mockStatus.mock.calls.length).toBeGreaterThan(initialCallCount);
    });

    it('stops polling on unmount', async () => {
      mockStatus.mockResolvedValue(mockNetworkStatus);

      const { unmount } = renderHook(() => useNetworkStatus());

      // Flush pending promises for initial fetch
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      const callCountBeforeUnmount = mockStatus.mock.calls.length;

      unmount();

      // Advance time - should not trigger more calls
      await act(async () => {
        await vi.advanceTimersByTimeAsync(60000);
      });

      // Call count should not increase after unmount
      expect(mockStatus.mock.calls.length).toBe(callCountBeforeUnmount);
    });
  });

  describe('refresh', () => {
    it('manually refreshes status', async () => {
      mockStatus.mockResolvedValue(mockNetworkStatus);

      const { result } = renderHook(() => useNetworkStatus());

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false);
      });

      const callCountBeforeRefresh = mockStatus.mock.calls.length;

      // Manual refresh
      await act(async () => {
        await result.current.refresh();
      });

      // Should have at least one more call after refresh
      expect(mockStatus.mock.calls.length).toBeGreaterThan(callCountBeforeRefresh);
    });

    it('sets loading state during refresh', async () => {
      let resolvePromise: (value: typeof mockNetworkStatus) => void;
      mockStatus.mockImplementation(
        () =>
          new Promise((resolve) => {
            resolvePromise = resolve;
          })
      );

      const { result } = renderHook(() => useNetworkStatus());

      // Wait for initial load
      await act(async () => {
        resolvePromise!(mockNetworkStatus);
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

  describe('default values', () => {
    it('returns defaults when status is null', () => {
      mockStatus.mockImplementation(() => new Promise(() => {}));

      const { result } = renderHook(() => useNetworkStatus());

      expect(result.current.isConnected).toBe(false);
      expect(result.current.peerCount).toBe(0);
      expect(result.current.peers).toEqual([]);
    });
  });
});
