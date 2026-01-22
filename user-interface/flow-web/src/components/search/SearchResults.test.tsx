import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SearchResults } from './SearchResults';
import { mockLocalResult, mockNetworkResult, mockSearchResponse } from '../../test/fixtures/search';
import type { SearchStats } from '../../hooks/useDistributedSearch';

const mockStats: SearchStats = {
  totalFound: mockSearchResponse.total_found,
  localCount: mockSearchResponse.local_count,
  networkCount: mockSearchResponse.network_count,
  peersQueried: mockSearchResponse.peers_queried,
  peersResponded: mockSearchResponse.peers_responded,
  elapsedMs: mockSearchResponse.elapsed_ms,
};

describe('SearchResults', () => {
  const mockOnLoadMore = vi.fn();
  const mockOnResultClick = vi.fn();
  const mockOnNetworkResultClick = vi.fn();

  const defaultProps = {
    results: [mockLocalResult, mockNetworkResult],
    stats: mockStats,
    hasMore: false,
    isLoading: false,
    onLoadMore: mockOnLoadMore,
    onResultClick: mockOnResultClick,
    onNetworkResultClick: mockOnNetworkResultClick,
  };

  beforeEach(() => {
    mockOnLoadMore.mockClear();
    mockOnResultClick.mockClear();
    mockOnNetworkResultClick.mockClear();
  });

  describe('rendering', () => {
    it('renders results list', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByRole('list')).toBeInTheDocument();
      expect(screen.getAllByRole('listitem')).toHaveLength(2);
    });

    it('renders both local and network results', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText('LOCAL')).toBeInTheDocument();
      expect(screen.getByText('NETWORK')).toBeInTheDocument();
    });

    it('returns null for empty results when not loading', () => {
      const { container } = render(
        <SearchResults {...defaultProps} results={[]} />
      );

      expect(container.firstChild).toBeNull();
    });
  });

  describe('stats display', () => {
    it('renders total result count', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText('2 results found')).toBeInTheDocument();
    });

    it('renders singular form for 1 result', () => {
      render(
        <SearchResults
          {...defaultProps}
          stats={{ ...mockStats, totalFound: 1 }}
        />
      );

      expect(screen.getByText('1 result found')).toBeInTheDocument();
    });

    it('renders local count', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText('1 local')).toBeInTheDocument();
    });

    it('renders network count', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText('1 network')).toBeInTheDocument();
    });

    it('renders elapsed time', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText(/245ms/)).toBeInTheDocument();
    });

    it('renders peer stats', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText(/2\/3 peers/)).toBeInTheDocument();
    });

    it('does not render peer stats when no peers queried', () => {
      render(
        <SearchResults
          {...defaultProps}
          stats={{ ...mockStats, peersQueried: 0 }}
        />
      );

      expect(screen.queryByText(/peers/)).not.toBeInTheDocument();
    });

    it('has live region for accessibility', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite');
    });
  });

  describe('load more', () => {
    it('renders load more button when hasMore is true', () => {
      render(<SearchResults {...defaultProps} hasMore />);

      expect(screen.getByText('Load more results')).toBeInTheDocument();
    });

    it('does not render load more button when hasMore is false', () => {
      render(<SearchResults {...defaultProps} hasMore={false} />);

      expect(screen.queryByText('Load more results')).not.toBeInTheDocument();
    });

    it('calls onLoadMore when load more button is clicked', async () => {
      const user = userEvent.setup();
      render(<SearchResults {...defaultProps} hasMore />);

      await user.click(screen.getByText('Load more results'));

      expect(mockOnLoadMore).toHaveBeenCalledTimes(1);
    });

    it('shows loading text and disables button when loading', () => {
      render(<SearchResults {...defaultProps} hasMore isLoading />);

      const button = screen.getByText('Loading...');
      expect(button).toBeInTheDocument();
      expect(button).toBeDisabled();
    });
  });

  describe('result click handlers', () => {
    it('calls onResultClick for local results', async () => {
      const user = userEvent.setup();
      render(<SearchResults {...defaultProps} />);

      // Find and click the local result card
      const localCard = screen.getByLabelText('Local result: Test Document');
      await user.click(localCard);

      expect(mockOnResultClick).toHaveBeenCalledWith(mockLocalResult);
    });

    it('network results are not directly clickable (handled by preview modal)', async () => {
      render(<SearchResults {...defaultProps} />);

      // Network result cards are articles, not buttons - they open via preview modal
      // The ResultCard intentionally doesn't make network results clickable
      const networkCard = screen.getByLabelText('Network result: Network Result');

      // Should be an article, not a button
      expect(networkCard.tagName).toBe('ARTICLE');
      expect(networkCard).not.toHaveAttribute('role', 'button');
    });

    it('does not call onResultClick for network results', async () => {
      const user = userEvent.setup();
      render(<SearchResults {...defaultProps} />);

      const networkCard = screen.getByLabelText('Network result: Network Result');
      await user.click(networkCard);

      expect(mockOnResultClick).not.toHaveBeenCalled();
    });
  });

  describe('stats breakdown', () => {
    it('shows separator between local and network counts', () => {
      render(<SearchResults {...defaultProps} />);

      expect(screen.getByText('·')).toBeInTheDocument();
    });

    it('does not show separator when only local results', () => {
      render(
        <SearchResults
          {...defaultProps}
          stats={{ ...mockStats, networkCount: 0 }}
        />
      );

      expect(screen.queryByText('1 network')).not.toBeInTheDocument();
    });

    it('does not show separator when only network results', () => {
      render(
        <SearchResults
          {...defaultProps}
          stats={{ ...mockStats, localCount: 0 }}
        />
      );

      expect(screen.queryByText(/local/)).not.toBeInTheDocument();
    });
  });

  describe('null stats', () => {
    it('does not render stats section when stats is null', () => {
      render(<SearchResults {...defaultProps} stats={null} />);

      expect(screen.queryByRole('status')).not.toBeInTheDocument();
    });
  });
});
