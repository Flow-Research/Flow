import { useState, useEffect, useCallback, useRef } from 'react';
import { api, SpaceStatus } from '../services/api';

/** Polling intervals in milliseconds */
const ACTIVE_POLL_INTERVAL = 5000; // 5 seconds when indexing
const IDLE_POLL_INTERVAL = 60000; // 60 seconds when idle

/** Return type for the useSpaceStatus hook */
export interface UseSpaceStatusReturn {
  /** Current space status */
  status: SpaceStatus | null;
  /** Whether status is loading */
  isLoading: boolean;
  /** Error message if fetch failed */
  error: string | null;
  /** Manually refresh status */
  refresh: () => Promise<void>;
  /** Trigger re-indexing */
  reindex: () => Promise<void>;
  /** Whether re-indexing is in progress */
  isReindexing: boolean;
}

/**
 * Custom hook for fetching and polling space indexing status.
 * Automatically adjusts polling frequency based on indexing state.
 *
 * @param spaceKey - The space key to fetch status for
 */
export function useSpaceStatus(spaceKey: string): UseSpaceStatusReturn {
  const [status, setStatus] = useState<SpaceStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [isReindexing, setIsReindexing] = useState(false);

  const intervalRef = useRef<number | null>(null);
  const mountedRef = useRef(true);

  const fetchStatus = useCallback(async () => {
    try {
      const data = await api.spaces.getStatus(spaceKey);
      if (mountedRef.current) {
        setStatus(data);
        setError(null);
      }
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Failed to fetch status');
      }
    } finally {
      if (mountedRef.current) {
        setIsLoading(false);
      }
    }
  }, [spaceKey]);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    await fetchStatus();
  }, [fetchStatus]);

  const reindex = useCallback(async () => {
    setIsReindexing(true);
    setError(null);

    try {
      await api.spaces.reindex(spaceKey);
      // Refresh status after triggering reindex
      await fetchStatus();
    } catch (err) {
      if (mountedRef.current) {
        setError(err instanceof Error ? err.message : 'Failed to trigger re-index');
      }
    } finally {
      if (mountedRef.current) {
        setIsReindexing(false);
      }
    }
  }, [spaceKey, fetchStatus]);

  // Set up polling with adaptive interval
  useEffect(() => {
    mountedRef.current = true;

    // Clear any existing interval
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }

    // Initial fetch
    fetchStatus();

    // Determine poll interval based on indexing state
    const pollInterval = status?.indexing_in_progress
      ? ACTIVE_POLL_INTERVAL
      : IDLE_POLL_INTERVAL;

    // Set up new interval
    intervalRef.current = window.setInterval(fetchStatus, pollInterval);

    return () => {
      mountedRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [spaceKey, status?.indexing_in_progress, fetchStatus]);

  return {
    status,
    isLoading,
    error,
    refresh,
    reindex,
    isReindexing,
  };
}
