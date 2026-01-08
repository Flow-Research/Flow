import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import App from './App';
import { AuthProvider } from './contexts/AuthContext';

function renderApp(initialEntries: string[] = ['/']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <AuthProvider>
        <App />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe('App Routing', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    localStorage.clear();
    globalThis.fetch = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve([]),
    });
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  describe('TC-FE-006: ProtectedRoute redirects unauthenticated users', () => {
    it('redirects to /auth when accessing protected route without authentication', async () => {
      renderApp(['/']);

      await waitFor(() => {
        expect(screen.getByText('Welcome to Flow')).toBeInTheDocument();
      });
    });

    it('redirects to /auth when accessing /settings without authentication', async () => {
      renderApp(['/settings']);

      await waitFor(() => {
        expect(screen.getByText('Welcome to Flow')).toBeInTheDocument();
      });
    });

    it('redirects to /auth when accessing /spaces/:key without authentication', async () => {
      renderApp(['/spaces/test-space']);

      await waitFor(() => {
        expect(screen.getByText('Welcome to Flow')).toBeInTheDocument();
      });
    });
  });

  describe('TC-FE-007: ProtectedRoute allows authenticated users', () => {
    beforeEach(() => {
      localStorage.setItem('token', 'valid-token');
      localStorage.setItem('did', 'did:test:user123');
    });

    it('renders home page when authenticated and accessing /', async () => {
      renderApp(['/']);

      await waitFor(() => {
        expect(screen.getByText('Your Spaces')).toBeInTheDocument();
      });
    });

    it('renders settings page when authenticated and accessing /settings', async () => {
      renderApp(['/settings']);

      await waitFor(() => {
        expect(screen.getByRole('heading', { level: 1, name: 'Settings' })).toBeInTheDocument();
      });
    });
  });

  describe('PublicRoute redirects authenticated users', () => {
    beforeEach(() => {
      localStorage.setItem('token', 'valid-token');
      localStorage.setItem('did', 'did:test:user123');
    });

    it('redirects to / when authenticated user accesses /auth', async () => {
      renderApp(['/auth']);

      await waitFor(() => {
        expect(screen.getByText('Your Spaces')).toBeInTheDocument();
      });
    });
  });

  describe('Loading state', () => {
    it('renders without error during authentication check', () => {
      renderApp(['/']);
      expect(document.body).toBeInTheDocument();
    });
  });

  describe('Catch-all route', () => {
    beforeEach(() => {
      localStorage.setItem('token', 'valid-token');
      localStorage.setItem('did', 'did:test:user123');
    });

    it('redirects unknown routes to home for authenticated users', async () => {
      renderApp(['/unknown-route']);

      await waitFor(() => {
        expect(screen.getByText('Your Spaces')).toBeInTheDocument();
      });
    });
  });
});
