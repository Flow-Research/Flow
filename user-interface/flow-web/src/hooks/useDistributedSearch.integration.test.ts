/**
 * Integration Tests for useDistributedSearch
 *
 * These tests use MSW to intercept real network requests,
 * testing the complete data flow from hook → API client → fetch → response.
 *
 * Key difference from unit tests:
 * - No vi.mock - tests real module implementations
 * - MSW intercepts at network level
 * - Validates full request/response cycle
 */

import { describe, it, expect } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { useDistributedSearch } from './useDistributedSearch';
import { server } from '../test/mocks/server';
import { mockSearchResponse } from '../test/fixtures/search';

const API_BASE = 'http://localhost:8080/api/v1';

describe('useDistributedSearch (Integration)', () => {
  describe('successful search flow', () => {
    it('executes a real search and returns results', async () => {
      const { result } = renderHook(() => useDistributedSearch());

      expect(result.current.state).toBe('idle');

      act(() => {
        result.current.search('test query', 'all');
      });

      // Should transition to loading
      expect(result.current.state).toBe('loading');

      // Wait for the debounce and API call
      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      // Verify results came through the real API client
      expect(result.current.results).toHaveLength(2);
      expect(result.current.results[0].cid).toBe(mockSearchResponse.results[0].cid);
      expect(result.current.stats).toEqual({
        localCount: mockSearchResponse.local_count,
        networkCount: mockSearchResponse.network_count,
        peersQueried: mockSearchResponse.peers_queried,
        peersResponded: mockSearchResponse.peers_responded,
        totalFound: mockSearchResponse.total_found,
        elapsedMs: mockSearchResponse.elapsed_ms,
      });
    });

    it('returns empty results for queries with no matches', async () => {
      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('nonexistent', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      expect(result.current.results).toHaveLength(0);
      expect(result.current.stats?.totalFound).toBe(0);
    });

    it('passes scope parameter correctly to API', async () => {
      let capturedRequest: { query: string; scope: string } | null = null;

      server.use(
        http.post(`${API_BASE}/search/distributed`, async ({ request }) => {
          capturedRequest = (await request.json()) as { query: string; scope: string };
          return HttpResponse.json(mockSearchResponse);
        })
      );

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'local');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      expect(capturedRequest).not.toBeNull();
      expect(capturedRequest!.scope).toBe('local');
    });
  });

  describe('error handling', () => {
    it('handles network errors gracefully', async () => {
      server.use(
        http.post(`${API_BASE}/search/distributed`, () => {
          return HttpResponse.error();
        })
      );

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('error');
        },
        { timeout: 1000 }
      );

      expect(result.current.error).toBe('Search failed. Please try again.');
    });

    it('handles 500 server errors', async () => {
      server.use(
        http.post(`${API_BASE}/search/distributed`, () => {
          return HttpResponse.json(
            { error: 'Internal server error' },
            { status: 500 }
          );
        })
      );

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('error');
        },
        { timeout: 1000 }
      );

      expect(result.current.error).toBeDefined();
    });

    it('handles 401 unauthorized errors', async () => {
      server.use(
        http.post(`${API_BASE}/search/distributed`, () => {
          return HttpResponse.json(
            { error: 'Unauthorized' },
            { status: 401 }
          );
        })
      );

      const { result } = renderHook(() => useDistributedSearch());

      act(() => {
        result.current.search('test', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('error');
        },
        { timeout: 1000 }
      );

      expect(result.current.error).toBeDefined();
    });
  });

  describe('debouncing', () => {
    it('debounces rapid search calls - only final query is sent', async () => {
      let capturedQueries: string[] = [];

      server.use(
        http.post(`${API_BASE}/search/distributed`, async ({ request }) => {
          const body = (await request.json()) as { query: string };
          capturedQueries.push(body.query);
          return HttpResponse.json(mockSearchResponse);
        })
      );

      const { result } = renderHook(() => useDistributedSearch());

      // Rapid fire multiple searches
      act(() => {
        result.current.search('a', 'all');
      });
      act(() => {
        result.current.search('ab', 'all');
      });
      act(() => {
        result.current.search('abc', 'all');
      });

      // Wait for debounce to complete and request to finish
      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      // Only one request should be made (the last one)
      expect(capturedQueries).toHaveLength(1);
      expect(capturedQueries[0]).toBe('abc'); // Only the final query
    });
  });

  describe('clear functionality', () => {
    it('clears results and resets state', async () => {
      const { result } = renderHook(() => useDistributedSearch());

      // Perform a search
      act(() => {
        result.current.search('test', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      expect(result.current.results).toHaveLength(2);

      // Clear
      act(() => {
        result.current.clear();
      });

      expect(result.current.state).toBe('idle');
      expect(result.current.results).toEqual([]);
      expect(result.current.stats).toBeNull();
      expect(result.current.error).toBeNull();
    });

    it('empty query clears results', async () => {
      const { result } = renderHook(() => useDistributedSearch());

      // Perform a search
      act(() => {
        result.current.search('test', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      // Search with empty query
      act(() => {
        result.current.search('', 'all');
      });

      expect(result.current.state).toBe('idle');
      expect(result.current.results).toEqual([]);
    });
  });

  describe('pagination', () => {
    it('loads more results when hasMore is true', async () => {
      // First response with more results available
      server.use(
        http.post(`${API_BASE}/search/distributed`, async ({ request }) => {
          const body = (await request.json()) as { offset?: number };
          const offset = body.offset || 0;

          if (offset === 0) {
            return HttpResponse.json({
              ...mockSearchResponse,
              total_found: 50, // More than current results
            });
          } else {
            // Second page
            return HttpResponse.json({
              ...mockSearchResponse,
              results: [
                {
                  ...mockSearchResponse.results[0],
                  cid: 'bafynewpage',
                  title: 'Page 2 Result',
                },
              ],
              total_found: 50,
            });
          }
        })
      );

      const { result } = renderHook(() => useDistributedSearch());

      // Initial search
      act(() => {
        result.current.search('test', 'all');
      });

      await waitFor(
        () => {
          expect(result.current.state).toBe('success');
        },
        { timeout: 1000 }
      );

      expect(result.current.hasMore).toBe(true);
      expect(result.current.results).toHaveLength(2);

      // Load more
      act(() => {
        result.current.loadMore();
      });

      await waitFor(
        () => {
          expect(result.current.results.length).toBeGreaterThan(2);
        },
        { timeout: 1000 }
      );
    });
  });
});
