import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { AuthPage } from './Auth';
import { AuthProvider } from '../contexts/AuthContext';

function renderAuthPage() {
  return render(
    <MemoryRouter>
      <AuthProvider>
        <AuthPage />
      </AuthProvider>
    </MemoryRouter>
  );
}

describe('AuthPage', () => {
  const originalFetch = globalThis.fetch;

  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  describe('TC-FE-008: renders registration and login options', () => {
    it('displays the welcome header', () => {
      renderAuthPage();

      expect(screen.getByText('Welcome to Flow')).toBeInTheDocument();
    });

    it('displays sign in button', () => {
      renderAuthPage();

      expect(screen.getByText('Sign In with Passkey')).toBeInTheDocument();
    });

    it('displays create account button', () => {
      renderAuthPage();

      expect(screen.getByText('Create New Account')).toBeInTheDocument();
    });

    it('displays both auth options visible on the page', () => {
      renderAuthPage();

      const signInButton = screen.getByText('Sign In with Passkey');
      const createAccountButton = screen.getByText('Create New Account');

      expect(signInButton).toBeVisible();
      expect(createAccountButton).toBeVisible();
    });

    it('displays informational text about passkeys', () => {
      renderAuthPage();

      expect(screen.getByText(/Flow uses passkeys/)).toBeInTheDocument();
      expect(screen.getByText(/Your passkey is stored securely/)).toBeInTheDocument();
    });

    it('has both buttons enabled by default', () => {
      renderAuthPage();

      const signInButton = screen.getByText('Sign In with Passkey');
      const createAccountButton = screen.getByText('Create New Account');

      expect(signInButton).not.toBeDisabled();
      expect(createAccountButton).not.toBeDisabled();
    });

    it('displays the "or" divider between buttons', () => {
      renderAuthPage();

      expect(screen.getByText('or')).toBeInTheDocument();
    });
  });
});
