import { render, RenderOptions } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { AuthProvider } from '../contexts/AuthContext';
import { NetworkProvider } from '../contexts/NetworkContext';

interface WrapperProps {
  children: React.ReactNode;
}

/**
 * Wrapper with all providers for testing.
 */
function AllProviders({ children }: WrapperProps) {
  return (
    <MemoryRouter>
      <AuthProvider>
        <NetworkProvider>
          {children}
        </NetworkProvider>
      </AuthProvider>
    </MemoryRouter>
  );
}

/**
 * Custom render function that wraps components with all necessary providers.
 */
export function renderWithProviders(
  ui: React.ReactElement,
  options?: Omit<RenderOptions, 'wrapper'>
) {
  return render(ui, { wrapper: AllProviders, ...options });
}

/**
 * Re-export everything from testing-library for convenience.
 */
export * from '@testing-library/react';
export { renderWithProviders as render };
