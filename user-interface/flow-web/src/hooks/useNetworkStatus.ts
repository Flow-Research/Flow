import { useState, useEffect, useCallback, useRef } from 'react';
import { api } from '../services/api';
import type { NetworkStatus, PeerInfo } from '../types/api';

/** Polling interval for network status (30 seconds) */
const POLL_INTERVAL = 30000;

/** Return type for the useNetworkStatus hook */
export interface UseNetworkStatusReturn {
  /** Whether connected to the network */
  isConnected: boolean;
  /** Number of connected peers */
  peerCount: number;
  /** List of connected peers */
  peers: PeerInfo[];
  /** Whether status is loading */
  isLoading: boolean;
  /** Error message if fetch failed */
  error: string | null;
  /** Manually refresh status */
  refresh: () => Promise<void>;
}

/**
 * Custom hook for fetching and polling network connection status.
 * Polls every 30 seconds while authenticated.
 */
export function useNetworkStatus(): UseNetworkStatusReturn {
  const [status, setStatus] = useState<NetworkStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const intervalRef = useRef<number | null>(null);
  const mountedRef = useRef(true);

  const fetchStatus = useCallback(async () => {
    try {
      const data = await api.network.status();
      if (mountedRef.current) {
        setStatus(data);
        setError(null);
      }
    } catch (err) {
      if (mountedRef.current) {
        // Don't show error for network issues - just show disconnected
        setStatus({ connected: false, peer_count: 0, peers: [] });
        setError(err instanceof Error ? err.message : 'Failed to fetch network status');
      }
    } finally {
      if (mountedRef.current) {
        setIsLoading(false);
      }
    }
  }, []);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    await fetchStatus();
  }, [fetchStatus]);

  // Set up polling
  useEffect(() => {
    mountedRef.current = true;

    // Initial fetch
    fetchStatus();

    // Set up polling interval
    intervalRef.current = window.setInterval(fetchStatus, POLL_INTERVAL);

    return () => {
      mountedRef.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [fetchStatus]);

  return {
    isConnected: status?.connected ?? false,
    peerCount: status?.peer_count ?? 0,
    peers: status?.peers ?? [],
    isLoading,
    error,
    refresh,
  };
}
