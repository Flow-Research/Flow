/**
 * MSW Request Handlers
 *
 * These handlers intercept network requests at the network level,
 * allowing us to test the real fetch implementation while controlling responses.
 *
 * Unlike vi.mock, MSW:
 * - Tests the real API client code path
 * - Validates request structure
 * - Can simulate network delays and errors
 * - Works identically in tests and browser dev tools
 */

import { http, HttpResponse, delay } from 'msw';
import {
  mockSearchResponse,
  mockEmptySearchResponse,
  mockNetworkStatus,
  mockSpaceStatusIdle,
  mockPublishSuccess,
} from '../fixtures/search';

const API_BASE = 'http://localhost:8080/api/v1';

// Default handlers - happy path responses
export const handlers = [
  // Distributed Search
  http.post(`${API_BASE}/search/distributed`, async ({ request }) => {
    const body = await request.json() as { query?: string; scope?: string };

    // Validate request structure
    if (!body.query || typeof body.query !== 'string') {
      return HttpResponse.json(
        { error: 'Invalid request: query is required' },
        { status: 400 }
      );
    }

    // Simulate realistic network delay
    await delay(50);

    // Return empty results for specific queries
    if (body.query === 'nonexistent') {
      return HttpResponse.json(mockEmptySearchResponse);
    }

    return HttpResponse.json({
      ...mockSearchResponse,
      query: body.query,
      scope: body.scope || 'all',
    });
  }),

  // Search Health
  http.get(`${API_BASE}/search/distributed/health`, () => {
    return HttpResponse.json({
      status: 'healthy',
      local_search: { available: true },
      network_search: { available: true, connected_peers: 3 },
    });
  }),

  // Network Status
  http.get(`${API_BASE}/network/status`, () => {
    return HttpResponse.json(mockNetworkStatus);
  }),

  // Space Status
  http.get(`${API_BASE}/spaces/:key/status`, () => {
    return HttpResponse.json(mockSpaceStatusIdle);
  }),

  // Publish Space
  http.post(`${API_BASE}/spaces/:key/publish`, () => {
    return HttpResponse.json(mockPublishSuccess);
  }),

  // Unpublish Space
  http.delete(`${API_BASE}/spaces/:key/publish`, () => {
    return HttpResponse.json({
      success: true,
      unpublished_at: new Date().toISOString(),
    });
  }),

  // Health Check
  http.get(`${API_BASE}/health`, () => {
    return HttpResponse.json({
      status: 'healthy',
      timestamp: new Date().toISOString(),
    });
  }),

  // List Spaces
  http.get(`${API_BASE}/spaces`, () => {
    return HttpResponse.json([
      {
        id: 1,
        key: 'space-abc123',
        name: 'Test Space',
        location: '/path/to/space',
        time_created: '2026-01-22T10:00:00Z',
        is_published: false,
      },
    ]);
  }),

  // Get Space
  http.get(`${API_BASE}/spaces/:key`, ({ params }) => {
    return HttpResponse.json({
      id: 1,
      key: params.key,
      name: 'Test Space',
      location: '/path/to/space',
      time_created: '2026-01-22T10:00:00Z',
      is_published: false,
    });
  }),
];

/**
 * Error handlers for testing error scenarios
 */
export const errorHandlers = {
  networkError: http.post(`${API_BASE}/search/distributed`, () => {
    return HttpResponse.error();
  }),

  serverError: http.post(`${API_BASE}/search/distributed`, () => {
    return HttpResponse.json(
      { error: 'Internal server error' },
      { status: 500 }
    );
  }),

  unauthorized: http.post(`${API_BASE}/search/distributed`, () => {
    return HttpResponse.json(
      { error: 'Unauthorized' },
      { status: 401 }
    );
  }),

  timeout: http.post(`${API_BASE}/search/distributed`, async () => {
    await delay(30000); // 30 second delay to simulate timeout
    return HttpResponse.json(mockSearchResponse);
  }),

  networkDisconnected: http.get(`${API_BASE}/network/status`, () => {
    return HttpResponse.json({
      connected: false,
      peer_count: 0,
      peers: [],
    });
  }),

  spaceIndexing: http.get(`${API_BASE}/spaces/:key/status`, () => {
    return HttpResponse.json({
      indexing_in_progress: true,
      last_indexed: '2026-01-22T11:00:00Z',
      files_indexed: 20,
      chunks_stored: 128,
      files_failed: 0,
      last_error: null,
    });
  }),

  publishFailed: http.post(`${API_BASE}/spaces/:key/publish`, () => {
    return HttpResponse.json(
      { success: false, error: 'Space must be indexed before publishing' },
      { status: 400 }
    );
  }),
};
