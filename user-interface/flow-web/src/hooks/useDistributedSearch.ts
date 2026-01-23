import { useState, useCallback, useRef, useEffect } from 'react';
import { api, ApiError } from '../services/api';
import type {
  SearchScope,
  SearchResult,
  DistributedSearchResponse,
} from '../types/api';

/** Search state */
export type SearchState = 'idle' | 'loading' | 'success' | 'error';

/** Search stats returned from the API */
export interface SearchStats {
  localCount: number;
  networkCount: number;
  peersQueried: number;
  peersResponded: number;
  totalFound: number;
  elapsedMs: number;
}

/** Return type for the useDistributedSearch hook */
export interface UseDistributedSearchReturn {
  /** Current search state */
  state: SearchState;
  /** Search results */
  results: SearchResult[];
  /** Search statistics */
  stats: SearchStats | null;
  /** Error message if search failed */
  error: string | null;
  /** Execute a search */
  search: (query: string, scope: SearchScope) => Promise<void>;
  /** Load more results (pagination) */
  loadMore: () => Promise<void>;
  /** Clear search results */
  clear: () => void;
  /** Whether more results are available */
  hasMore: boolean;
}

const RESULTS_PER_PAGE = 20;
const SEARCH_DEBOUNCE_MS = 300;

/**
 * Custom hook for distributed search functionality.
 * Handles search execution, pagination, and error states.
 */
export function useDistributedSearch(): UseDistributedSearchReturn {
  const [state, setState] = useState<SearchState>('idle');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [stats, setStats] = useState<SearchStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);

  // Track current search params for pagination
  const currentQueryRef = useRef<string>('');
  const currentScopeRef = useRef<SearchScope>('all');
  const offsetRef = useRef<number>(0);

  // Debounce timer
  const debounceTimerRef = useRef<number | null>(null);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }
    };
  }, []);

  const executeSearch = useCallback(
    async (query: string, scope: SearchScope, offset: number) => {
      try {
        const response: DistributedSearchResponse = await api.search.distributed({
          query,
          scope,
          limit: RESULTS_PER_PAGE,
          offset,
        });

        const newStats: SearchStats = {
          localCount: response.local_count,
          networkCount: response.network_count,
          peersQueried: response.peers_queried,
          peersResponded: response.peers_responded,
          totalFound: response.total_found,
          elapsedMs: response.elapsed_ms,
        };

        return { results: response.results, stats: newStats };
      } catch (err) {
        if (err instanceof ApiError) {
          if (err.status === 400) {
            throw new Error('Invalid search query');
          } else if (err.status === 429) {
            throw new Error('Too many searches. Please wait a moment.');
          } else if (err.status === 503) {
            throw new Error('Search service unavailable');
          }
        }
        throw new Error('Search failed. Please try again.');
      }
    },
    []
  );

  const search = useCallback(
    async (query: string, scope: SearchScope) => {
      // Clear any pending debounced search
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
      }

      const trimmedQuery = query.trim();

      // Clear results for empty query
      if (!trimmedQuery) {
        setState('idle');
        setResults([]);
        setStats(null);
        setError(null);
        setHasMore(false);
        return;
      }

      // Update refs for pagination
      currentQueryRef.current = trimmedQuery;
      currentScopeRef.current = scope;
      offsetRef.current = 0;

      setState('loading');
      setError(null);

      // Debounce the search
      debounceTimerRef.current = window.setTimeout(async () => {
        try {
          const { results: newResults, stats: newStats } = await executeSearch(
            trimmedQuery,
            scope,
            0
          );

          setResults(newResults);
          setStats(newStats);
          setHasMore(newResults.length < newStats.totalFound);
          setState('success');
        } catch (err) {
          setError(err instanceof Error ? err.message : 'Search failed');
          setState('error');
        }
      }, SEARCH_DEBOUNCE_MS);
    },
    [executeSearch]
  );

  const loadMore = useCallback(async () => {
    if (state === 'loading' || !hasMore) return;

    const newOffset = offsetRef.current + RESULTS_PER_PAGE;
    offsetRef.current = newOffset;

    setState('loading');

    try {
      const { results: newResults, stats: newStats } = await executeSearch(
        currentQueryRef.current,
        currentScopeRef.current,
        newOffset
      );

      setResults((prev) => [...prev, ...newResults]);
      setStats(newStats);
      setHasMore(newOffset + newResults.length < newStats.totalFound);
      setState('success');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load more results');
      setState('error');
    }
  }, [state, hasMore, executeSearch]);

  const clear = useCallback(() => {
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }
    setState('idle');
    setResults([]);
    setStats(null);
    setError(null);
    setHasMore(false);
    currentQueryRef.current = '';
    offsetRef.current = 0;
  }, []);

  return {
    state,
    results,
    stats,
    error,
    search,
    loadMore,
    clear,
    hasMore,
  };
}
