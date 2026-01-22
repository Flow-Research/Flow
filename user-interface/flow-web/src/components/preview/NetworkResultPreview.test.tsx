import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { NetworkResultPreview } from './NetworkResultPreview';
import type { SearchResult } from '../../types/api';

describe('NetworkResultPreview', () => {
  const mockOnClose = vi.fn();

  const mockResult: SearchResult = {
    cid: 'bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi',
    score: 0.92,
    source: 'network',
    title: 'Test Network Result',
    snippet: 'This is a test snippet from the network...',
    source_id: '12D3KooWAbCdEfGhIjKlMnOpQrStUvWxYz',
  };

  beforeEach(() => {
    mockOnClose.mockClear();
  });

  describe('rendering', () => {
    it('renders nothing when result is null', () => {
      const { container } = render(
        <NetworkResultPreview
          result={null}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(container.firstChild).toBeNull();
    });

    it('renders modal with result data when open', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText('Network Result')).toBeInTheDocument();
      expect(screen.getByText('Test Network Result')).toBeInTheDocument();
      expect(screen.getByText('This is a test snippet from the network...')).toBeInTheDocument();
    });

    it('displays NETWORK badge', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText('NETWORK')).toBeInTheDocument();
    });

    it('displays score as percentage', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText('92% match')).toBeInTheDocument();
    });

    it('displays CID', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText(mockResult.cid)).toBeInTheDocument();
    });

    it('displays peer ID when available', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText(mockResult.source_id!)).toBeInTheDocument();
    });

    it('shows "Untitled" when title is not provided', () => {
      const resultWithoutTitle = { ...mockResult, title: undefined };
      render(
        <NetworkResultPreview
          result={resultWithoutTitle}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText('Untitled')).toBeInTheDocument();
    });

    it('does not render snippet section when snippet is not provided', () => {
      const resultWithoutSnippet = { ...mockResult, snippet: undefined };
      render(
        <NetworkResultPreview
          result={resultWithoutSnippet}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.queryByText('Content Preview')).not.toBeInTheDocument();
    });

    it('does not render peer ID section when source_id is not provided', () => {
      const resultWithoutSourceId = { ...mockResult, source_id: undefined };
      render(
        <NetworkResultPreview
          result={resultWithoutSourceId}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.queryByText('Publisher Peer ID')).not.toBeInTheDocument();
    });

    it('displays network note', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByText(/This content is published on the Flow Network/)).toBeInTheDocument();
    });
  });

  describe('copy functionality', () => {
    it('renders copy CID button that can be clicked', async () => {
      const user = userEvent.setup();
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      const copyCidButton = screen.getByLabelText('Copy CID');
      expect(copyCidButton).toBeInTheDocument();

      // Click should not throw - the handler will execute
      await user.click(copyCidButton);
    });

    it('renders copy peer ID button that can be clicked', async () => {
      const user = userEvent.setup();
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      const copyPeerIdButton = screen.getByLabelText('Copy Peer ID');
      expect(copyPeerIdButton).toBeInTheDocument();

      // Click should not throw - the handler will execute
      await user.click(copyPeerIdButton);
    });

    it('does not render copy buttons when result is null', () => {
      const { container } = render(
        <NetworkResultPreview
          result={null}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      // No buttons should be rendered since nothing is rendered
      expect(container.querySelector('button')).toBeNull();
    });

    it('does not render peer ID copy button when source_id is missing', () => {
      const resultWithoutSourceId = { ...mockResult, source_id: undefined };
      render(
        <NetworkResultPreview
          result={resultWithoutSourceId}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      // When source_id is undefined, the peer ID copy button is not rendered
      expect(screen.queryByLabelText('Copy Peer ID')).not.toBeInTheDocument();
      // But CID copy button should still be there
      expect(screen.getByLabelText('Copy CID')).toBeInTheDocument();
    });

    it('copy CID button has type="button" to prevent form submission', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      const copyCidButton = screen.getByLabelText('Copy CID');
      expect(copyCidButton).toHaveAttribute('type', 'button');
    });

    it('copy peer ID button has type="button" to prevent form submission', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      const copyPeerIdButton = screen.getByLabelText('Copy Peer ID');
      expect(copyPeerIdButton).toHaveAttribute('type', 'button');
    });
  });

  describe('modal behavior', () => {
    it('calls onClose when modal is closed', async () => {
      const user = userEvent.setup();
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      const closeButton = screen.getByLabelText('Close modal');
      await user.click(closeButton);

      expect(mockOnClose).toHaveBeenCalledTimes(1);
    });

    it('uses large modal size', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(document.querySelector('.modal--lg')).toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has accessible copy buttons', () => {
      render(
        <NetworkResultPreview
          result={mockResult}
          isOpen={true}
          onClose={mockOnClose}
        />
      );

      expect(screen.getByLabelText('Copy CID')).toBeInTheDocument();
      expect(screen.getByLabelText('Copy Peer ID')).toBeInTheDocument();
    });
  });
});
