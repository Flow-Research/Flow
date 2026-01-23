import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { api, ApiError } from './api';

describe('API Client', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  describe('TC-FE-004: includes auth header', () => {
    it('includes Authorization header when token exists in localStorage', async () => {
      localStorage.setItem('token', 'test-bearer-token');

      let capturedHeaders: HeadersInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((_url: string, options?: RequestInit) => {
        capturedHeaders = options?.headers;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok', timestamp: new Date().toISOString() }),
        });
      });

      await api.health.check();

      expect(capturedHeaders).toBeDefined();
      expect((capturedHeaders as Record<string, string>)['Authorization']).toBe('Bearer test-bearer-token');
    });

    it('does not include Authorization header when no token exists', async () => {
      let capturedHeaders: HeadersInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((_url: string, options?: RequestInit) => {
        capturedHeaders = options?.headers;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok', timestamp: new Date().toISOString() }),
        });
      });

      await api.health.check();

      expect(capturedHeaders).toBeDefined();
      expect((capturedHeaders as Record<string, string>)['Authorization']).toBeUndefined();
    });

    it('always includes Content-Type header', async () => {
      let capturedHeaders: HeadersInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((_url: string, options?: RequestInit) => {
        capturedHeaders = options?.headers;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok', timestamp: new Date().toISOString() }),
        });
      });

      await api.health.check();

      expect(capturedHeaders).toBeDefined();
      expect((capturedHeaders as Record<string, string>)['Content-Type']).toBe('application/json');
    });
  });

  describe('TC-FE-005: handles errors correctly', () => {
    it('throws ApiError with correct status code on non-ok response', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        text: () => Promise.resolve('Resource not found'),
      });

      await expect(api.health.check()).rejects.toThrow(ApiError);

      try {
        await api.health.check();
      } catch (error) {
        expect(error).toBeInstanceOf(ApiError);
        expect((error as ApiError).status).toBe(404);
        expect((error as ApiError).message).toBe('Resource not found');
      }
    });

    it('uses statusText when error text is empty', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        text: () => Promise.resolve(''),
      });

      try {
        await api.health.check();
      } catch (error) {
        expect(error).toBeInstanceOf(ApiError);
        expect((error as ApiError).status).toBe(500);
        expect((error as ApiError).message).toBe('Internal Server Error');
      }
    });

    it('throws ApiError with 401 status for unauthorized requests', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
        statusText: 'Unauthorized',
        text: () => Promise.resolve('Invalid token'),
      });

      try {
        await api.spaces.list();
      } catch (error) {
        expect(error).toBeInstanceOf(ApiError);
        expect((error as ApiError).status).toBe(401);
      }
    });
  });

  describe('ApiError class', () => {
    it('has correct name property', () => {
      const error = new ApiError(400, 'Bad Request');
      expect(error.name).toBe('ApiError');
    });

    it('is instanceof Error', () => {
      const error = new ApiError(400, 'Bad Request');
      expect(error).toBeInstanceOf(Error);
    });
  });

  describe('API endpoints', () => {
    it('calls correct URL for spaces.list', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve([]),
        });
      });

      await api.spaces.list();

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces');
    });

    it('calls correct URL for spaces.get with key', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ id: 1, key: 'test', location: '/test', time_created: '' }),
        });
      });

      await api.spaces.get('test-key');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/test-key');
    });

    it('calls correct URL for spaces.create with POST method', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok' }),
        });
      });

      await api.spaces.create('/path/to/dir');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces');
      expect(capturedOptions?.method).toBe('POST');
      expect(capturedOptions?.body).toBe(JSON.stringify({ dir: '/path/to/dir' }));
    });

    it('calls correct URL for spaces.delete with DELETE method', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok' }),
        });
      });

      await api.spaces.delete('test-key');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/test-key');
      expect(capturedOptions?.method).toBe('DELETE');
    });

    it('calls correct URL for spaces.getStatus', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            indexing_in_progress: false,
            last_indexed: null,
            files_indexed: 0,
            chunks_stored: 0,
            files_failed: 0,
            last_error: null,
          }),
        });
      });

      await api.spaces.getStatus('test-key');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/test-key/status');
    });

    it('calls correct URL for spaces.reindex with POST method', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({ status: 'ok' }),
        });
      });

      await api.spaces.reindex('test-key');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/test-key/reindex');
      expect(capturedOptions?.method).toBe('POST');
    });
  });

  describe('search API', () => {
    it('calls distributed search with correct URL and body', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            success: true,
            query: 'test query',
            scope: 'all',
            results: [],
            local_count: 0,
            network_count: 0,
            peers_queried: 0,
            peers_responded: 0,
            total_found: 0,
            elapsed_ms: 100,
          }),
        });
      });

      await api.search.distributed({
        query: 'test query',
        scope: 'all',
        limit: 20,
        offset: 0,
      });

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/search/distributed');
      expect(capturedOptions?.method).toBe('POST');
      const body = JSON.parse(capturedOptions?.body as string);
      expect(body.query).toBe('test query');
      expect(body.scope).toBe('all');
      expect(body.limit).toBe(20);
      expect(body.offset).toBe(0);
    });

    it('calls distributed search with local scope', async () => {
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((_url: string, options?: RequestInit) => {
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            success: true,
            query: 'local search',
            scope: 'local',
            results: [],
            local_count: 0,
            network_count: 0,
            peers_queried: 0,
            peers_responded: 0,
            total_found: 0,
            elapsed_ms: 50,
          }),
        });
      });

      await api.search.distributed({
        query: 'local search',
        scope: 'local',
      });

      const body = JSON.parse(capturedOptions?.body as string);
      expect(body.scope).toBe('local');
    });

    it('calls distributed search with network scope', async () => {
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((_url: string, options?: RequestInit) => {
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            success: true,
            query: 'network search',
            scope: 'network',
            results: [],
            local_count: 0,
            network_count: 0,
            peers_queried: 3,
            peers_responded: 2,
            total_found: 0,
            elapsed_ms: 200,
          }),
        });
      });

      await api.search.distributed({
        query: 'network search',
        scope: 'network',
      });

      const body = JSON.parse(capturedOptions?.body as string);
      expect(body.scope).toBe('network');
    });

    it('calls search health endpoint', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            local_search_ready: true,
            network_search_ready: true,
            peer_count: 3,
          }),
        });
      });

      await api.search.health();

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/search/distributed/health');
    });
  });

  describe('publish API', () => {
    it('calls publish with correct URL and POST method', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            success: true,
            cid: 'bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi',
            published_at: '2026-01-22T12:00:00Z',
          }),
        });
      });

      await api.publish.publish('my-space');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/my-space/publish');
      expect(capturedOptions?.method).toBe('POST');
    });

    it('calls unpublish with correct URL and DELETE method', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            success: true,
            unpublished_at: '2026-01-22T13:00:00Z',
          }),
        });
      });

      await api.publish.unpublish('my-space');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/my-space/publish');
      expect(capturedOptions?.method).toBe('DELETE');
    });
  });

  describe('network API', () => {
    it('calls network status with correct URL', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            connected: true,
            peer_count: 3,
            peers: [
              { id: '12D3KooWtest1', connected_at: '2026-01-22T10:00:00Z' },
            ],
          }),
        });
      });

      await api.network.status();

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/network/status');
    });

    it('returns network status response correctly', async () => {
      const mockResponse = {
        connected: true,
        peer_count: 3,
        peers: [
          { id: '12D3KooWtest1', connected_at: '2026-01-22T10:00:00Z' },
          { id: '12D3KooWtest2', connected_at: '2026-01-22T09:45:00Z' },
        ],
      };

      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      });

      const result = await api.network.status();

      expect(result.connected).toBe(true);
      expect(result.peer_count).toBe(3);
      expect(result.peers).toHaveLength(2);
    });
  });

  describe('query API', () => {
    it('calls search with correct URL and encoded parameters', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            status: 'ok',
            response: 'Test response',
          }),
        });
      });

      await api.query.search('test-space', 'what is AI?');

      expect(capturedUrl).toContain('/spaces/search');
      expect(capturedUrl).toContain('space_key=test-space');
      expect(capturedUrl).toContain('query=what%20is%20AI%3F');
    });
  });

  describe('entities API', () => {
    it('calls entities.list with correct URL', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve([]),
        });
      });

      await api.entities.list('test-space');

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/test-space/entities');
    });

    it('calls entities.get with correct URL', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            id: 123,
            cid: 'bafytest',
            name: 'Test Entity',
            entity_type: 'document',
            properties: {},
            created_at: '2026-01-22T12:00:00Z',
          }),
        });
      });

      await api.entities.get('test-space', 123);

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/spaces/test-space/entities/123');
    });
  });

  describe('auth API', () => {
    it('calls startRegistration with correct URL', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            challenge: {},
            challenge_id: 'test-challenge',
          }),
        });
      });

      await api.auth.startRegistration();

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/webauthn/start_registration');
    });

    it('calls finishRegistration with POST method and body', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            verified: true,
            token: 'test-token',
            did: 'did:test:123',
          }),
        });
      });

      await api.auth.finishRegistration('challenge-123', { id: 'cred' });

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/webauthn/finish_registration');
      expect(capturedOptions?.method).toBe('POST');
      const body = JSON.parse(capturedOptions?.body as string);
      expect(body.challenge_id).toBe('challenge-123');
      expect(body.credential).toEqual({ id: 'cred' });
    });

    it('calls startAuthentication with POST method', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            challenge: {},
            challenge_id: 'auth-challenge',
          }),
        });
      });

      await api.auth.startAuthentication();

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/webauthn/start_authentication');
      expect(capturedOptions?.method).toBe('POST');
    });

    it('calls finishAuthentication with POST method and body', async () => {
      let capturedUrl: string | undefined;
      let capturedOptions: RequestInit | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string, options?: RequestInit) => {
        capturedUrl = url;
        capturedOptions = options;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            verified: true,
            token: 'auth-token',
            did: 'did:test:456',
          }),
        });
      });

      await api.auth.finishAuthentication('auth-challenge-456', { assertion: 'data' });

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/webauthn/finish_authentication');
      expect(capturedOptions?.method).toBe('POST');
      const body = JSON.parse(capturedOptions?.body as string);
      expect(body.challenge_id).toBe('auth-challenge-456');
      expect(body.credential).toEqual({ assertion: 'data' });
    });
  });

  describe('health API', () => {
    it('calls health check with correct URL', async () => {
      let capturedUrl: string | undefined;

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        capturedUrl = url;
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve({
            status: 'ok',
            timestamp: '2026-01-22T12:00:00Z',
          }),
        });
      });

      await api.health.check();

      expect(capturedUrl).toBe('http://localhost:8080/api/v1/health');
    });

    it('returns health check response correctly', async () => {
      const mockResponse = {
        status: 'ok',
        timestamp: '2026-01-22T12:00:00Z',
      };

      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockResponse),
      });

      const result = await api.health.check();

      expect(result.status).toBe('ok');
      expect(result.timestamp).toBe('2026-01-22T12:00:00Z');
    });
  });
});
