import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, act, waitFor } from '@testing-library/react';
import { AuthProvider, useAuth } from './AuthContext';

function AuthTestConsumer() {
  const auth = useAuth();
  return (
    <div>
      <span data-testid="isLoading">{String(auth.isLoading)}</span>
      <span data-testid="isAuthenticated">{String(auth.isAuthenticated)}</span>
      <span data-testid="token">{auth.token || 'null'}</span>
      <span data-testid="user-did">{auth.user?.did || 'null'}</span>
      <button onClick={() => auth.login('test-token', 'did:test:123')}>Login</button>
      <button onClick={() => auth.logout()}>Logout</button>
    </div>
  );
}

describe('AuthContext', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  describe('TC-FE-001: provides authentication state', () => {
    it('provides isLoading as true initially then false after mount', async () => {
      render(
        <AuthProvider>
          <AuthTestConsumer />
        </AuthProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('isLoading').textContent).toBe('false');
      });
    });

    it('provides isAuthenticated as false when no token exists', async () => {
      render(
        <AuthProvider>
          <AuthTestConsumer />
        </AuthProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('isLoading').textContent).toBe('false');
      });
      expect(screen.getByTestId('isAuthenticated').textContent).toBe('false');
    });

    it('provides isAuthenticated as true when token exists in localStorage', async () => {
      localStorage.setItem('token', 'existing-token');
      localStorage.setItem('did', 'did:existing:456');

      render(
        <AuthProvider>
          <AuthTestConsumer />
        </AuthProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('isLoading').textContent).toBe('false');
      });
      expect(screen.getByTestId('isAuthenticated').textContent).toBe('true');
      expect(screen.getByTestId('token').textContent).toBe('existing-token');
      expect(screen.getByTestId('user-did').textContent).toBe('did:existing:456');
    });
  });

  describe('TC-FE-002: login flow updates state', () => {
    it('stores token and updates isAuthenticated when login is called', async () => {
      render(
        <AuthProvider>
          <AuthTestConsumer />
        </AuthProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('isLoading').textContent).toBe('false');
      });

      expect(screen.getByTestId('isAuthenticated').textContent).toBe('false');

      await act(async () => {
        screen.getByText('Login').click();
      });

      expect(screen.getByTestId('isAuthenticated').textContent).toBe('true');
      expect(screen.getByTestId('token').textContent).toBe('test-token');
      expect(screen.getByTestId('user-did').textContent).toBe('did:test:123');

      expect(localStorage.getItem('token')).toBe('test-token');
      expect(localStorage.getItem('did')).toBe('did:test:123');
    });
  });

  describe('TC-FE-003: logout clears state', () => {
    it('clears token and resets authentication state when logout is called', async () => {
      localStorage.setItem('token', 'existing-token');
      localStorage.setItem('did', 'did:existing:456');

      render(
        <AuthProvider>
          <AuthTestConsumer />
        </AuthProvider>
      );

      await waitFor(() => {
        expect(screen.getByTestId('isLoading').textContent).toBe('false');
      });

      expect(screen.getByTestId('isAuthenticated').textContent).toBe('true');

      await act(async () => {
        screen.getByText('Logout').click();
      });

      expect(screen.getByTestId('isAuthenticated').textContent).toBe('false');
      expect(screen.getByTestId('token').textContent).toBe('null');
      expect(screen.getByTestId('user-did').textContent).toBe('null');

      expect(localStorage.getItem('token')).toBeNull();
      expect(localStorage.getItem('did')).toBeNull();
    });
  });

  describe('useAuth hook', () => {
    it('throws error when used outside AuthProvider', () => {
      const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

      expect(() => {
        render(<AuthTestConsumer />);
      }).toThrow('useAuth must be used within an AuthProvider');

      consoleSpy.mockRestore();
    });
  });
});
