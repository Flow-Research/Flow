import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { NetworkIndicator } from './NetworkIndicator';
import * as NetworkContext from '../../contexts/NetworkContext';

// Mock the useNetwork hook
vi.mock('../../contexts/NetworkContext', () => ({
  useNetwork: vi.fn(),
}));

const mockUseNetwork = vi.mocked(NetworkContext.useNetwork);

describe('NetworkIndicator', () => {
  const mockRefresh = vi.fn();
  const mockOnClick = vi.fn();

  const connectedState = {
    isConnected: true,
    peerCount: 3,
    peers: [],
    isLoading: false,
    error: null,
    refresh: mockRefresh,
  };

  const disconnectedState = {
    isConnected: false,
    peerCount: 0,
    peers: [],
    isLoading: false,
    error: null,
    refresh: mockRefresh,
  };

  const loadingState = {
    isConnected: false,
    peerCount: 0,
    peers: [],
    isLoading: true,
    error: null,
    refresh: mockRefresh,
  };

  beforeEach(() => {
    mockRefresh.mockClear();
    mockOnClick.mockClear();
    mockUseNetwork.mockReturnValue(connectedState);
  });

  describe('connected state', () => {
    it('renders peer count when connected', () => {
      render(<NetworkIndicator />);

      expect(screen.getByText('3 peers')).toBeInTheDocument();
    });

    it('renders singular peer when count is 1', () => {
      mockUseNetwork.mockReturnValue({ ...connectedState, peerCount: 1 });
      render(<NetworkIndicator />);

      expect(screen.getByText('1 peer')).toBeInTheDocument();
    });

    it('renders Connected when showPeerCount is false', () => {
      render(<NetworkIndicator showPeerCount={false} />);

      expect(screen.getByText('Connected')).toBeInTheDocument();
    });

    it('has connected status class', () => {
      render(<NetworkIndicator />);

      expect(document.querySelector('.network-indicator--connected')).toBeInTheDocument();
    });

    it('shows connected dot', () => {
      render(<NetworkIndicator />);

      expect(document.querySelector('.network-indicator__dot--connected')).toBeInTheDocument();
    });
  });

  describe('disconnected state', () => {
    beforeEach(() => {
      mockUseNetwork.mockReturnValue(disconnectedState);
    });

    it('renders Offline when disconnected', () => {
      render(<NetworkIndicator />);

      expect(screen.getByText('Offline')).toBeInTheDocument();
    });

    it('has disconnected status class', () => {
      render(<NetworkIndicator />);

      expect(document.querySelector('.network-indicator--disconnected')).toBeInTheDocument();
    });

    it('shows disconnected dot', () => {
      render(<NetworkIndicator />);

      expect(document.querySelector('.network-indicator__dot--disconnected')).toBeInTheDocument();
    });
  });

  describe('loading state', () => {
    beforeEach(() => {
      mockUseNetwork.mockReturnValue(loadingState);
    });

    it('renders ... when loading', () => {
      render(<NetworkIndicator />);

      expect(screen.getByText('...')).toBeInTheDocument();
    });

    it('has unknown status class', () => {
      render(<NetworkIndicator />);

      expect(document.querySelector('.network-indicator--unknown')).toBeInTheDocument();
    });
  });

  describe('click behavior', () => {
    it('renders as button when onClick is provided', () => {
      render(<NetworkIndicator onClick={mockOnClick} />);

      expect(screen.getByRole('button')).toBeInTheDocument();
    });

    it('renders as div when onClick is not provided', () => {
      render(<NetworkIndicator />);

      expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });

    it('calls onClick when clicked', async () => {
      const user = userEvent.setup();
      render(<NetworkIndicator onClick={mockOnClick} />);

      await user.click(screen.getByRole('button'));

      expect(mockOnClick).toHaveBeenCalledTimes(1);
    });

    it('has clickable class when onClick is provided', () => {
      render(<NetworkIndicator onClick={mockOnClick} />);

      expect(document.querySelector('.network-indicator--clickable')).toBeInTheDocument();
    });

    it('does not have clickable class when onClick is not provided', () => {
      render(<NetworkIndicator />);

      expect(document.querySelector('.network-indicator--clickable')).not.toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has aria-label when clickable', () => {
      render(<NetworkIndicator onClick={mockOnClick} />);

      expect(screen.getByRole('button')).toHaveAttribute(
        'aria-label',
        'Network status: 3 peers. Click for details.'
      );
    });

    it('has correct aria-label when disconnected', () => {
      mockUseNetwork.mockReturnValue(disconnectedState);
      render(<NetworkIndicator onClick={mockOnClick} />);

      expect(screen.getByRole('button')).toHaveAttribute(
        'aria-label',
        'Network status: Offline. Click for details.'
      );
    });

    it('dot is hidden from screen readers', () => {
      render(<NetworkIndicator />);

      const dot = document.querySelector('.network-indicator__dot');
      expect(dot).toHaveAttribute('aria-hidden', 'true');
    });
  });
});
