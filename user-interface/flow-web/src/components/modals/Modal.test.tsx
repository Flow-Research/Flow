import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Modal } from './Modal';

describe('Modal', () => {
  const mockOnClose = vi.fn();
  const originalOverflow = document.body.style.overflow;

  beforeEach(() => {
    mockOnClose.mockClear();
  });

  afterEach(() => {
    document.body.style.overflow = originalOverflow;
  });

  describe('rendering', () => {
    it('renders modal content when open', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Modal content</p>
        </Modal>
      );

      expect(screen.getByText('Test Modal')).toBeInTheDocument();
      expect(screen.getByText('Modal content')).toBeInTheDocument();
    });

    it('does not render when closed', () => {
      render(
        <Modal isOpen={false} onClose={mockOnClose} title="Test Modal">
          <p>Modal content</p>
        </Modal>
      );

      expect(screen.queryByText('Test Modal')).not.toBeInTheDocument();
    });

    it('renders close button', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(screen.getByLabelText('Close modal')).toBeInTheDocument();
    });
  });

  describe('closing behavior', () => {
    it('calls onClose when close button is clicked', async () => {
      const user = userEvent.setup();
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      await user.click(screen.getByLabelText('Close modal'));

      expect(mockOnClose).toHaveBeenCalledTimes(1);
    });

    it('calls onClose when backdrop is clicked', async () => {
      const user = userEvent.setup();
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      const backdrop = document.querySelector('.modal-backdrop');
      if (backdrop) {
        await user.click(backdrop);
      }

      expect(mockOnClose).toHaveBeenCalledTimes(1);
    });

    it('does not close when clicking inside modal', async () => {
      const user = userEvent.setup();
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      await user.click(screen.getByText('Content'));

      expect(mockOnClose).not.toHaveBeenCalled();
    });

    it('calls onClose when Escape key is pressed', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      fireEvent.keyDown(document, { key: 'Escape' });

      expect(mockOnClose).toHaveBeenCalledTimes(1);
    });
  });

  describe('body scroll lock', () => {
    it('prevents body scroll when modal opens', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(document.body.style.overflow).toBe('hidden');
    });

    it('restores body scroll when modal closes', () => {
      const { rerender } = render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(document.body.style.overflow).toBe('hidden');

      rerender(
        <Modal isOpen={false} onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(document.body.style.overflow).toBe('');
    });
  });

  describe('sizes', () => {
    it('applies sm class for small width', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal" maxWidth="sm">
          <p>Content</p>
        </Modal>
      );

      expect(document.querySelector('.modal--sm')).toBeInTheDocument();
    });

    it('applies md class by default', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(document.querySelector('.modal--md')).toBeInTheDocument();
    });

    it('applies lg class for large width', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal" maxWidth="lg">
          <p>Content</p>
        </Modal>
      );

      expect(document.querySelector('.modal--lg')).toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has dialog role', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });

    it('has aria-modal attribute', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      expect(screen.getByRole('dialog')).toHaveAttribute('aria-modal', 'true');
    });

    it('has aria-labelledby pointing to title', () => {
      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      const dialog = screen.getByRole('dialog');
      expect(dialog).toHaveAttribute('aria-labelledby', 'modal-title');
      expect(screen.getByText('Test Modal')).toHaveAttribute('id', 'modal-title');
    });

    it('focuses the modal when opened', async () => {
      vi.useFakeTimers();

      render(
        <Modal isOpen onClose={mockOnClose} title="Test Modal">
          <p>Content</p>
        </Modal>
      );

      await act(async () => {
        vi.advanceTimersByTime(0);
      });

      expect(screen.getByRole('dialog')).toHaveFocus();

      vi.useRealTimers();
    });
  });
});
