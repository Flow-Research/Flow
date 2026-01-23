import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { PublishButton } from './PublishButton';
import { api } from '../../services/api';

// Mock the API
vi.mock('../../services/api', () => ({
  api: {
    publish: {
      publish: vi.fn(),
      unpublish: vi.fn(),
    },
  },
}));

describe('PublishButton', () => {
  const mockOnStatusChange = vi.fn();
  const mockPublish = vi.mocked(api.publish.publish);
  const mockUnpublish = vi.mocked(api.publish.unpublish);

  const defaultProps = {
    spaceKey: 'my-space',
    spaceName: 'My Space',
    isPublished: false,
    onStatusChange: mockOnStatusChange,
  };

  beforeEach(() => {
    mockOnStatusChange.mockClear();
    mockPublish.mockClear();
    mockUnpublish.mockClear();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('unpublished state', () => {
    it('renders Publish button when not published', () => {
      render(<PublishButton {...defaultProps} />);

      expect(screen.getByText('Publish')).toBeInTheDocument();
    });

    it('opens confirmation modal on click', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} />);

      await user.click(screen.getByText('Publish'));

      expect(screen.getByText('Publish Space')).toBeInTheDocument();
    });

    it('has aria-pressed false when not published', () => {
      render(<PublishButton {...defaultProps} />);

      expect(screen.getByRole('button', { name: 'Publish' })).toHaveAttribute('aria-pressed', 'false');
    });
  });

  describe('published state', () => {
    it('renders Published button when published', () => {
      render(<PublishButton {...defaultProps} isPublished />);

      expect(screen.getByText('Published')).toBeInTheDocument();
    });

    it('shows status dot when published', () => {
      render(<PublishButton {...defaultProps} isPublished />);

      expect(document.querySelector('.publish-button__status-dot')).toBeInTheDocument();
    });

    it('has aria-pressed true when published', () => {
      render(<PublishButton {...defaultProps} isPublished />);

      expect(screen.getByRole('button', { name: /Published/ })).toHaveAttribute('aria-pressed', 'true');
    });

    it('opens dropdown on click', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      await user.click(screen.getByText('Published'));

      expect(screen.getByRole('menu')).toBeInTheDocument();
    });

    it('displays published date in dropdown', async () => {
      const user = userEvent.setup();
      render(
        <PublishButton
          {...defaultProps}
          isPublished
          publishedAt="2026-01-22T12:00:00Z"
        />
      );

      await user.click(screen.getByText('Published'));

      expect(screen.getByText(/Published Jan 22, 2026/)).toBeInTheDocument();
    });

    it('shows unpublish option in dropdown', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      await user.click(screen.getByText('Published'));

      expect(screen.getByRole('menuitem', { name: 'Unpublish' })).toBeInTheDocument();
    });

    it('closes dropdown when backdrop is clicked', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      await user.click(screen.getByText('Published'));
      expect(screen.getByRole('menu')).toBeInTheDocument();

      const backdrop = document.querySelector('.publish-button__backdrop');
      if (backdrop) {
        await user.click(backdrop);
      }

      expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    });
  });

  describe('publish action', () => {
    it('calls API to publish when confirmed', async () => {
      const user = userEvent.setup();
      mockPublish.mockResolvedValue({
        success: true,
        cid: 'bafytest123',
        published_at: '2026-01-22T12:00:00Z',
      });

      render(<PublishButton {...defaultProps} />);

      // Open modal
      await user.click(screen.getByText('Publish'));

      // Confirm publish
      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(mockPublish).toHaveBeenCalledWith('my-space');
      });
    });

    it('calls onStatusChange with new status after publish', async () => {
      const user = userEvent.setup();
      mockPublish.mockResolvedValue({
        success: true,
        cid: 'bafytest123',
        published_at: '2026-01-22T12:00:00Z',
      });

      render(<PublishButton {...defaultProps} />);

      await user.click(screen.getByText('Publish'));
      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(mockOnStatusChange).toHaveBeenCalledWith(true, '2026-01-22T12:00:00Z');
      });
    });

    it('shows error when publish fails', async () => {
      const user = userEvent.setup();
      mockPublish.mockResolvedValue({
        success: false,
        error: 'Space not indexed',
      });

      render(<PublishButton {...defaultProps} />);

      await user.click(screen.getByText('Publish'));
      await user.click(screen.getByText('Publish to Network'));

      await waitFor(() => {
        expect(screen.getByRole('alert')).toHaveTextContent('Space not indexed');
      });
    });
  });

  describe('unpublish action', () => {
    it('opens unpublish modal when Unpublish is clicked', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      // Open dropdown
      await user.click(screen.getByText('Published'));
      // Click unpublish
      await user.click(screen.getByRole('menuitem', { name: 'Unpublish' }));

      expect(screen.getByText('Unpublish Space')).toBeInTheDocument();
    });

    it('calls API to unpublish when confirmed', async () => {
      const user = userEvent.setup();
      mockUnpublish.mockResolvedValue({ success: true });

      render(<PublishButton {...defaultProps} isPublished />);

      // Open dropdown
      await user.click(screen.getByText('Published'));
      // Click unpublish
      await user.click(screen.getByRole('menuitem', { name: 'Unpublish' }));
      // Confirm in modal
      await user.click(screen.getByRole('button', { name: 'Unpublish' }));

      await waitFor(() => {
        expect(mockUnpublish).toHaveBeenCalledWith('my-space');
      });
    });

    it('calls onStatusChange with false after unpublish', async () => {
      const user = userEvent.setup();
      mockUnpublish.mockResolvedValue({ success: true });

      render(<PublishButton {...defaultProps} isPublished />);

      await user.click(screen.getByText('Published'));
      await user.click(screen.getByRole('menuitem', { name: 'Unpublish' }));
      await user.click(screen.getByRole('button', { name: 'Unpublish' }));

      await waitFor(() => {
        expect(mockOnStatusChange).toHaveBeenCalledWith(false);
      });
    });

    it('closes dropdown when unpublish modal opens', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      await user.click(screen.getByText('Published'));
      expect(screen.getByRole('menu')).toBeInTheDocument();

      await user.click(screen.getByRole('menuitem', { name: 'Unpublish' }));

      expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has aria-haspopup when published', () => {
      render(<PublishButton {...defaultProps} isPublished />);

      expect(screen.getByRole('button', { name: /Published/ })).toHaveAttribute('aria-haspopup', 'menu');
    });

    it('has aria-expanded when dropdown is open', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      const button = screen.getByRole('button', { name: /Published/ });
      expect(button).toHaveAttribute('aria-expanded', 'false');

      await user.click(button);

      expect(button).toHaveAttribute('aria-expanded', 'true');
    });

    it('dropdown items have menuitem role', async () => {
      const user = userEvent.setup();
      render(<PublishButton {...defaultProps} isPublished />);

      await user.click(screen.getByText('Published'));

      expect(screen.getByRole('menuitem', { name: 'Unpublish' })).toBeInTheDocument();
    });
  });
});
