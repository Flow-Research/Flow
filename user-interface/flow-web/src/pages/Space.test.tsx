import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Routes, Route } from 'react-router-dom';
import { SpacePage } from './Space';
import { AuthProvider } from '../contexts/AuthContext';

function renderSpacePage(spaceKey: string = 'test-space') {
  return render(
    <MemoryRouter initialEntries={[`/spaces/${spaceKey}`]}>
      <AuthProvider>
        <Routes>
          <Route path="/spaces/:spaceKey" element={<SpacePage />} />
        </Routes>
      </AuthProvider>
    </MemoryRouter>
  );
}

describe('SpacePage', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem('token', 'test-token');
    localStorage.setItem('did', 'did:test:123');
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  describe('TC-FE-010: loads space details', () => {
    it('shows loading state initially', () => {
      globalThis.fetch = vi.fn().mockImplementation(() => new Promise(() => {}));

      renderSpacePage();

      expect(screen.getByText('Loading space...')).toBeInTheDocument();
    });

    it('displays space key as title', async () => {
      const mockSpace = { id: 1, key: 'my-documents', location: '/home/user/docs', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: '2024-01-15T12:00:00Z', files_indexed: 42, chunks_stored: 156, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage('my-documents');

      await waitFor(() => {
        expect(screen.getByText('my-documents')).toBeInTheDocument();
      });
    });

    it('displays space stats', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: '2024-01-15T12:00:00Z', files_indexed: 42, chunks_stored: 156, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByText('42')).toBeInTheDocument();
        expect(screen.getByText('156')).toBeInTheDocument();
      });
    });

    it('displays stat labels', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: '2024-01-15T12:00:00Z', files_indexed: 42, chunks_stored: 156, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByText('Files')).toBeInTheDocument();
        expect(screen.getByText('Chunks')).toBeInTheDocument();
        expect(screen.getByText('Last Indexed')).toBeInTheDocument();
      });
    });

    it('shows indexing badge when indexing is in progress', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: true, last_indexed: null, files_indexed: 10, chunks_stored: 25, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByText('Indexing...')).toBeInTheDocument();
      });
    });

    it('displays chat and entities tabs', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: null, files_indexed: 0, chunks_stored: 0, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Chat/ })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /Entities/ })).toBeInTheDocument();
      });
    });

    it('displays delete button', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: null, files_indexed: 0, chunks_stored: 0, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByText('Delete')).toBeInTheDocument();
      });
    });

    it('displays back button', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: null, files_indexed: 0, chunks_stored: 0, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByRole('button', { name: '←' })).toBeInTheDocument();
      });
    });

    it('handles missing space data gracefully', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        text: () => Promise.resolve('Not found'),
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByText('test-space')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /Chat/ })).toBeInTheDocument();
      });
    });

    it('shows "Never" for last indexed when null', async () => {
      const mockSpace = { id: 1, key: 'test-space', location: '/test', time_created: '2024-01-15T10:00:00Z' };
      const mockStatus = { indexing_in_progress: false, last_indexed: null, files_indexed: 0, chunks_stored: 0, files_failed: 0, last_error: null };

      globalThis.fetch = vi.fn().mockImplementation((url: string) => {
        if (url.includes('/status')) {
          return Promise.resolve({
            ok: true,
            json: () => Promise.resolve(mockStatus),
          });
        }
        return Promise.resolve({
          ok: true,
          json: () => Promise.resolve(mockSpace),
        });
      });

      renderSpacePage();

      await waitFor(() => {
        expect(screen.getByText('Never')).toBeInTheDocument();
      });
    });
  });
});
