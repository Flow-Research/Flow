import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { HomePage } from './Home';
import { AuthProvider } from '../contexts/AuthContext';

function renderHomePage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <HomePage />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe('HomePage', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem('token', 'test-token');
    localStorage.setItem('did', 'did:test:123');
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  describe('TC-FE-009: displays space list', () => {
    it('shows loading state initially', () => {
      globalThis.fetch = vi.fn().mockImplementation(() => new Promise(() => {}));

      renderHomePage();

      expect(screen.getByText('Loading spaces...')).toBeInTheDocument();
    });

    it('displays spaces when API returns data', async () => {
      const mockSpaces = [
        { id: 1, key: 'project-docs', location: '/home/user/docs', time_created: '2024-01-15T10:00:00Z' },
        { id: 2, key: 'notes', location: '/home/user/notes', time_created: '2024-01-16T11:00:00Z' },
      ];

      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpaces),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('project-docs')).toBeInTheDocument();
        expect(screen.getByText('notes')).toBeInTheDocument();
      });
    });

    it('displays space locations', async () => {
      const mockSpaces = [
        { id: 1, key: 'project-docs', location: '/home/user/docs', time_created: '2024-01-15T10:00:00Z' },
      ];

      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpaces),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('/home/user/docs')).toBeInTheDocument();
      });
    });

    it('displays empty state when no spaces exist', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve([]),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('No spaces yet')).toBeInTheDocument();
        expect(screen.getByText('Create your first space to start indexing documents.')).toBeInTheDocument();
      });
    });

    it('displays "Your Spaces" header', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve([]),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('Your Spaces')).toBeInTheDocument();
      });
    });

    it('displays create button in header', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve([]),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('+ Create Space')).toBeInTheDocument();
      });
    });

    it('handles 404 error gracefully by showing empty state', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 404,
        statusText: 'Not Found',
        text: () => Promise.resolve('404'),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('No spaces yet')).toBeInTheDocument();
      });
    });

    it('displays error message when API fails with non-404 error', async () => {
      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
        statusText: 'Internal Server Error',
        text: () => Promise.resolve('Server error'),
      });

      renderHomePage();

      await waitFor(() => {
        expect(screen.getByText('Server error')).toBeInTheDocument();
      });
    });

    it('renders space cards as links', async () => {
      const mockSpaces = [
        { id: 1, key: 'project-docs', location: '/home/user/docs', time_created: '2024-01-15T10:00:00Z' },
      ];

      globalThis.fetch = vi.fn().mockResolvedValue({
        ok: true,
        json: () => Promise.resolve(mockSpaces),
      });

      renderHomePage();

      await waitFor(() => {
        const link = screen.getByRole('link', { name: /project-docs/ });
        expect(link).toHaveAttribute('href', '/spaces/project-docs');
      });
    });
  });
});
