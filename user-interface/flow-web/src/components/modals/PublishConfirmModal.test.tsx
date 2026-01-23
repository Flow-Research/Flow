import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PublishConfirmModal } from './PublishConfirmModal';

describe('PublishConfirmModal', () => {
  const mockOnClose = vi.fn();
  const mockOnConfirm = vi.fn();

  const defaultProps = {
    isOpen: true,
    onClose: mockOnClose,
    spaceName: 'my-space',
    onConfirm: mockOnConfirm,
  };

  beforeEach(() => {
    mockOnClose.mockClear();
    mockOnConfirm.mockClear().mockResolvedValue(undefined);
  });

  describe('publish mode rendering', () => {
    it('renders publish title', () => {
      render(<PublishConfirmModal {...defaultProps} />);

      expect(screen.getByText('Publish Space')).toBeInTheDocument();
    });

    it('renders space name', () => {
      render(<PublishConfirmModal {...defaultProps} />);

      expect(screen.getByText(/my-space/)).toBeInTheDocument();
    });

    it('renders privacy notice', () => {
      render(<PublishConfirmModal {...defaultProps} />);

      expect(screen.getByText('⚠️ Privacy Notice')).toBeInTheDocument();
    });

    it('renders what gets shared section', () => {
      render(<PublishConfirmModal {...defaultProps} />);

      expect(screen.getByText('What gets shared:')).toBeInTheDocument();
      expect(screen.getByText(/Document titles and content snippets/)).toBeInTheDocument();
    });

    it('renders publish button', () => {
      render(<PublishConfirmModal {...defaultProps} />);

      expect(screen.getByText('Publish to Network')).toBeInTheDocument();
    });

    it('renders cancel button', () => {
      render(<PublishConfirmModal {...defaultProps} />);

      expect(screen.getByText('Cancel')).toBeInTheDocument();
    });
  });

  describe('unpublish mode rendering', () => {
    it('renders unpublish title', () => {
      render(<PublishConfirmModal {...defaultProps} isUnpublish />);

      expect(screen.getByText('Unpublish Space')).toBeInTheDocument();
    });

    it('renders unpublish description', () => {
      render(<PublishConfirmModal {...defaultProps} isUnpublish />);

      expect(screen.getByText(/Are you sure you want to unpublish/)).toBeInTheDocument();
    });

    it('renders what happens when you unpublish', () => {
      render(<PublishConfirmModal {...defaultProps} isUnpublish />);

      expect(screen.getByText('What happens when you unpublish:')).toBeInTheDocument();
      expect(screen.getByText(/will no longer appear in network search/)).toBeInTheDocument();
    });

    it('renders unpublish button', () => {
      render(<PublishConfirmModal {...defaultProps} isUnpublish />);

      expect(screen.getByText('Unpublish')).toBeInTheDocument();
    });

    it('applies danger styling to unpublish button', () => {
      render(<PublishConfirmModal {...defaultProps} isUnpublish />);

      const unpublishBtn = screen.getByText('Unpublish');
      expect(unpublishBtn).toHaveClass('publish-confirm__btn--danger');
    });
  });

  describe('confirm action', () => {
    it('calls onConfirm when publish button is clicked', async () => {
      const user = userEvent.setup();
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      expect(mockOnConfirm).toHaveBeenCalledTimes(1);
    });

    it('calls onClose after successful confirm', async () => {
      const user = userEvent.setup();
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(mockOnClose).toHaveBeenCalledTimes(1);
      });
    });

    it('shows loading state while confirming', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockImplementation(() => new Promise(() => {})); // Never resolves
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      expect(screen.getByText('Publishing...')).toBeInTheDocument();
    });

    it('shows unpublishing loading state', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockImplementation(() => new Promise(() => {}));
      render(<PublishConfirmModal {...defaultProps} isUnpublish />);

      await user.click(screen.getByText('Unpublish'));

      expect(screen.getByText('Unpublishing...')).toBeInTheDocument();
    });

    it('disables buttons while loading', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockImplementation(() => new Promise(() => {}));
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      expect(screen.getByText('Publishing...')).toBeDisabled();
      expect(screen.getByText('Cancel')).toBeDisabled();
    });
  });

  describe('error handling', () => {
    it('displays error message on failure', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockRejectedValue(new Error('Network error'));
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('Network error');
      });
    });

    it('displays generic error for non-Error rejections', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockRejectedValue('Something went wrong');
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('Operation failed');
      });
    });

    it('does not close modal on error', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockRejectedValue(new Error('Failed'));
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(screen.getByRole('alert')).toBeInTheDocument();
      });

      expect(mockOnClose).not.toHaveBeenCalled();
    });

    it('re-enables buttons after error', async () => {
      const user = userEvent.setup();
      mockOnConfirm.mockRejectedValue(new Error('Failed'));
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(screen.getByText('Publish to Network')).not.toBeDisabled();
        expect(screen.getByText('Cancel')).not.toBeDisabled();
      });
    });
  });

  describe('cancel action', () => {
    it('calls onClose when cancel button is clicked', async () => {
      const user = userEvent.setup();
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Cancel'));

      expect(mockOnClose).toHaveBeenCalledTimes(1);
    });

    it('does not call onConfirm when cancelled', async () => {
      const user = userEvent.setup();
      render(<PublishConfirmModal {...defaultProps} />);

      await user.click(screen.getByText('Cancel'));

      expect(mockOnConfirm).not.toHaveBeenCalled();
    });
  });

  describe('closed state', () => {
    it('does not render when closed', () => {
      render(<PublishConfirmModal {...defaultProps} isOpen={false} />);

      expect(screen.queryByText('Publish Space')).not.toBeInTheDocument();
    });
  });
});
