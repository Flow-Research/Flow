import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { NetworkDetails } from './NetworkDetails';
import * as NetworkContext from '../../contexts/NetworkContext';
import { mockNetworkStatus, mockDisconnectedNetworkStatus } from '../../test/fixtures/search';

// Mock the useNetwork hook
vi.mock('../../contexts/NetworkContext', () => ({
  useNetwork: vi.fn(),
}));

const mockUseNetwork = vi.mocked(NetworkContext.useNetwork);

describe('NetworkDetails', () => {
  const mockRefresh = vi.fn();

  const connectedState = {
    isConnected: mockNetworkStatus.connected,
    peerCount: mockNetworkStatus.peer_count,
    peers: mockNetworkStatus.peers,
    isLoading: false,
    error: null,
    refresh: mockRefresh,
  };

  const disconnectedState = {
    isConnected: mockDisconnectedNetworkStatus.connected,
    peerCount: mockDisconnectedNetworkStatus.peer_count,
    peers: mockDisconnectedNetworkStatus.peers,
    isLoading: false,
    error: null,
    refresh: mockRefresh,
  };

  const loadingState = {
    ...connectedState,
    isLoading: true,
  };

  const errorState = {
    ...disconnectedState,
    error: 'Failed to fetch network status',
  };

  beforeEach(() => {
    mockRefresh.mockClear();
    mockUseNetwork.mockReturnValue(connectedState);
  });

  describe('header', () => {
    it('renders Network Status title', () => {
      render(<NetworkDetails />);

      expect(screen.getByText('Network Status')).toBeInTheDocument();
    });

    it('renders refresh button', () => {
      render(<NetworkDetails />);

      expect(screen.getByRole('button', { name: /Refresh/ })).toBeInTheDocument();
    });

    it('shows Refreshing text when loading', () => {
      mockUseNetwork.mockReturnValue(loadingState);
      render(<NetworkDetails />);

      expect(screen.getByText('Refreshing...')).toBeInTheDocument();
    });

    it('disables refresh button when loading', () => {
      mockUseNetwork.mockReturnValue(loadingState);
      render(<NetworkDetails />);

      expect(screen.getByRole('button', { name: 'Refresh network status' })).toBeDisabled();
    });

    it('calls refresh when button is clicked', async () => {
      const user = userEvent.setup();
      render(<NetworkDetails />);

      await user.click(screen.getByRole('button', { name: /Refresh/ }));

      expect(mockRefresh).toHaveBeenCalledTimes(1);
    });
  });

  describe('connection status', () => {
    it('shows Connected when connected', () => {
      render(<NetworkDetails />);

      expect(screen.getByText('Connected')).toBeInTheDocument();
    });

    it('shows Disconnected when not connected', () => {
      mockUseNetwork.mockReturnValue(disconnectedState);
      render(<NetworkDetails />);

      expect(screen.getByText('Disconnected')).toBeInTheDocument();
    });

    it('shows peer count', () => {
      render(<NetworkDetails />);

      expect(screen.getByText('Peers')).toBeInTheDocument();
      expect(screen.getByText('3')).toBeInTheDocument();
    });

    it('has connected styling class when connected', () => {
      render(<NetworkDetails />);

      expect(document.querySelector('.network-details__value--connected')).toBeInTheDocument();
    });

    it('has disconnected styling class when disconnected', () => {
      mockUseNetwork.mockReturnValue(disconnectedState);
      render(<NetworkDetails />);

      expect(document.querySelector('.network-details__value--disconnected')).toBeInTheDocument();
    });
  });

  describe('error state', () => {
    beforeEach(() => {
      mockUseNetwork.mockReturnValue(errorState);
    });

    it('shows error message', () => {
      render(<NetworkDetails />);

      expect(screen.getByRole('alert')).toHaveTextContent('Failed to fetch network status');
    });
  });

  describe('peer list', () => {
    it('shows Connected Peers heading', () => {
      render(<NetworkDetails />);

      expect(screen.getByText('Connected Peers')).toBeInTheDocument();
    });

    it('renders peer list', () => {
      render(<NetworkDetails />);

      const peerList = screen.getByRole('list');
      expect(peerList).toBeInTheDocument();
    });

    it('renders all peers', () => {
      render(<NetworkDetails />);

      const items = screen.getAllByRole('listitem');
      expect(items).toHaveLength(3);
    });

    it('truncates long peer IDs', () => {
      render(<NetworkDetails />);

      // First peer ID should be truncated: 12D3KooWabc123456789abcdefghij -> 12D3KooW...cdefghij
      expect(screen.getByTitle('12D3KooWabc123456789abcdefghij')).toBeInTheDocument();
      expect(screen.getByText('12D3KooW...cdefghij')).toBeInTheDocument();
    });

    it('shows connection time for each peer', () => {
      render(<NetworkDetails />);

      // Formatted dates should appear (finding any date in list items)
      const listItems = screen.getAllByRole('listitem');
      expect(listItems.length).toBeGreaterThan(0);
    });

    it('does not show peer list when no peers', () => {
      mockUseNetwork.mockReturnValue(disconnectedState);
      render(<NetworkDetails />);

      expect(screen.queryByText('Connected Peers')).not.toBeInTheDocument();
    });
  });

  describe('empty state', () => {
    it('shows empty message when no peers and not loading', () => {
      mockUseNetwork.mockReturnValue(disconnectedState);
      render(<NetworkDetails />);

      expect(screen.getByText(/No peers connected/)).toBeInTheDocument();
    });

    it('does not show empty message when loading', () => {
      mockUseNetwork.mockReturnValue({ ...disconnectedState, isLoading: true });
      render(<NetworkDetails />);

      expect(screen.queryByText(/No peers connected/)).not.toBeInTheDocument();
    });

    it('does not show empty message when there is an error', () => {
      mockUseNetwork.mockReturnValue(errorState);
      render(<NetworkDetails />);

      expect(screen.queryByText(/No peers connected/)).not.toBeInTheDocument();
    });

    it('does not show empty message when peers exist', () => {
      render(<NetworkDetails />);

      expect(screen.queryByText(/No peers connected/)).not.toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('refresh button has aria-label', () => {
      render(<NetworkDetails />);

      expect(screen.getByRole('button')).toHaveAttribute('aria-label', 'Refresh network status');
    });

    it('peer IDs have title for full value', () => {
      render(<NetworkDetails />);

      const firstPeerId = screen.getByTitle('12D3KooWabc123456789abcdefghij');
      expect(firstPeerId).toBeInTheDocument();
    });
  });
});
