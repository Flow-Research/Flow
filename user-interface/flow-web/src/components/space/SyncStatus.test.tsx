import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SyncStatus } from './SyncStatus';
import { mockSpaceStatusIdle, mockSpaceStatusIndexing, mockSpaceStatusWithError } from '../../test/fixtures/search';

describe('SyncStatus', () => {
  const mockOnReindex = vi.fn();
  const mockOnRefresh = vi.fn();

  const defaultProps = {
    status: mockSpaceStatusIdle,
    isLoading: false,
    error: null,
    isReindexing: false,
    onReindex: mockOnReindex,
    onRefresh: mockOnRefresh,
  };

  beforeEach(() => {
    mockOnReindex.mockClear();
    mockOnRefresh.mockClear();
  });

  describe('loading state', () => {
    it('shows skeleton when loading without status', () => {
      render(<SyncStatus {...defaultProps} status={null} isLoading />);

      expect(document.querySelector('.sync-status__skeleton')).toBeInTheDocument();
    });

    it('shows status when loading with existing status', () => {
      render(<SyncStatus {...defaultProps} isLoading />);

      expect(screen.getByText('Synced')).toBeInTheDocument();
    });
  });

  describe('error state', () => {
    it('shows error message', () => {
      render(<SyncStatus {...defaultProps} status={null} error="Network error" />);

      expect(screen.getByText('Network error')).toBeInTheDocument();
    });

    it('shows retry button on error', () => {
      render(<SyncStatus {...defaultProps} status={null} error="Failed" />);

      expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    });

    it('calls onRefresh when retry is clicked', async () => {
      const user = userEvent.setup();
      render(<SyncStatus {...defaultProps} status={null} error="Failed" />);

      await user.click(screen.getByRole('button', { name: 'Retry' }));

      expect(mockOnRefresh).toHaveBeenCalledTimes(1);
    });
  });

  describe('null status', () => {
    it('returns null when no status and not loading', () => {
      const { container } = render(<SyncStatus {...defaultProps} status={null} />);

      expect(container.firstChild).toBeNull();
    });
  });

  describe('synced state', () => {
    it('shows Synced badge', () => {
      render(<SyncStatus {...defaultProps} />);

      expect(screen.getByText('Synced')).toBeInTheDocument();
    });

    it('shows checkmark in synced state', () => {
      render(<SyncStatus {...defaultProps} />);

      expect(screen.getByText('✓')).toBeInTheDocument();
    });

    it('shows files count', () => {
      render(<SyncStatus {...defaultProps} />);

      expect(screen.getByText('42')).toBeInTheDocument();
      expect(screen.getByText('Files')).toBeInTheDocument();
    });

    it('shows chunks count', () => {
      render(<SyncStatus {...defaultProps} />);

      expect(screen.getByText('256')).toBeInTheDocument();
      expect(screen.getByText('Chunks')).toBeInTheDocument();
    });

    it('shows formatted last indexed date', () => {
      render(<SyncStatus {...defaultProps} />);

      expect(screen.getByText('Last Indexed')).toBeInTheDocument();
      // Check that a date-like value is shown
      expect(screen.getByText(/Jan 22/)).toBeInTheDocument();
    });

    it('shows Re-index button when showReindex is true', () => {
      render(<SyncStatus {...defaultProps} />);

      expect(screen.getByRole('button', { name: /Re-index/ })).toBeInTheDocument();
    });

    it('hides Re-index button when showReindex is false', () => {
      render(<SyncStatus {...defaultProps} showReindex={false} />);

      expect(screen.queryByRole('button', { name: /Re-index/ })).not.toBeInTheDocument();
    });

    it('calls onReindex when Re-index button is clicked', async () => {
      const user = userEvent.setup();
      render(<SyncStatus {...defaultProps} />);

      await user.click(screen.getByRole('button', { name: /Re-index/ }));

      expect(mockOnReindex).toHaveBeenCalledTimes(1);
    });
  });

  describe('indexing state', () => {
    it('shows Indexing badge when indexing_in_progress', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusIndexing} />);

      expect(screen.getByText('Indexing')).toBeInTheDocument();
    });

    it('shows spinner when indexing', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusIndexing} />);

      expect(document.querySelector('.sync-status__spinner')).toBeInTheDocument();
    });

    it('shows Indexing badge when isReindexing', () => {
      render(<SyncStatus {...defaultProps} isReindexing />);

      expect(screen.getByText('Indexing')).toBeInTheDocument();
    });

    it('hides Re-index button during indexing', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusIndexing} />);

      expect(screen.queryByRole('button', { name: /Re-index/ })).not.toBeInTheDocument();
    });

    it('shows SyncProgress component during indexing', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusIndexing} />);

      expect(screen.getByText('Processing files...')).toBeInTheDocument();
    });
  });

  describe('error status (from status object)', () => {
    it('shows Error badge when status has last_error', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusWithError} />);

      expect(screen.getByText('Error')).toBeInTheDocument();
    });

    it('shows exclamation in error state', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusWithError} />);

      expect(screen.getByText('!')).toBeInTheDocument();
    });

    it('shows failed count when files_failed > 0', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusWithError} />);

      expect(screen.getByText('7')).toBeInTheDocument();
      expect(screen.getByText('Failed')).toBeInTheDocument();
    });

    it('shows error details expandable section', () => {
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusWithError} />);

      expect(screen.getByText('View last error')).toBeInTheDocument();
    });

    it('shows error message in expanded details', async () => {
      const user = userEvent.setup();
      render(<SyncStatus {...defaultProps} status={mockSpaceStatusWithError} />);

      await user.click(screen.getByText('View last error'));

      expect(screen.getByText(/Failed to process file: document.pdf/)).toBeInTheDocument();
    });
  });

  describe('never indexed state', () => {
    it('shows Never when last_indexed is null', () => {
      const status = { ...mockSpaceStatusIdle, last_indexed: null };
      render(<SyncStatus {...defaultProps} status={status} />);

      expect(screen.getByText('Never')).toBeInTheDocument();
    });
  });
});
