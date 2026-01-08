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
  });
});
