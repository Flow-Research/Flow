import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SyncProgress } from './SyncProgress';

describe('SyncProgress', () => {
  const defaultProps = {
    filesIndexed: 10,
    chunksStored: 50,
    filesFailed: 0,
  };

  describe('rendering', () => {
    it('renders processing message', () => {
      render(<SyncProgress {...defaultProps} />);

      expect(screen.getByText('Processing files...')).toBeInTheDocument();
    });

    it('renders files indexed count', () => {
      render(<SyncProgress {...defaultProps} />);

      expect(screen.getByText(/10 indexed/)).toBeInTheDocument();
    });

    it('renders chunks count when > 0', () => {
      render(<SyncProgress {...defaultProps} />);

      expect(screen.getByText(/50 chunks/)).toBeInTheDocument();
    });

    it('does not render chunks count when 0', () => {
      render(<SyncProgress {...defaultProps} chunksStored={0} />);

      expect(screen.queryByText(/chunks/)).not.toBeInTheDocument();
    });

    it('renders failed count when > 0', () => {
      render(<SyncProgress {...defaultProps} filesFailed={3} />);

      expect(screen.getByText(/3 failed/)).toBeInTheDocument();
    });

    it('does not render failed count when 0', () => {
      render(<SyncProgress {...defaultProps} filesFailed={0} />);

      expect(screen.queryByText(/failed/)).not.toBeInTheDocument();
    });

    it('renders progress bar', () => {
      render(<SyncProgress {...defaultProps} />);

      expect(document.querySelector('.sync-progress__bar')).toBeInTheDocument();
      expect(document.querySelector('.sync-progress__bar-fill')).toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has status role', () => {
      render(<SyncProgress {...defaultProps} />);

      expect(screen.getByRole('status')).toBeInTheDocument();
    });

    it('has aria-live polite', () => {
      render(<SyncProgress {...defaultProps} />);

      expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite');
    });
  });

  describe('different values', () => {
    it('handles zero files indexed', () => {
      render(<SyncProgress filesIndexed={0} chunksStored={0} filesFailed={0} />);

      expect(screen.getByText(/0 indexed/)).toBeInTheDocument();
    });

    it('handles large numbers', () => {
      render(<SyncProgress filesIndexed={1234} chunksStored={5678} filesFailed={0} />);

      expect(screen.getByText(/1234 indexed/)).toBeInTheDocument();
      expect(screen.getByText(/5678 chunks/)).toBeInTheDocument();
    });

    it('shows all stats together', () => {
      render(<SyncProgress filesIndexed={100} chunksStored={500} filesFailed={5} />);

      // Stats are spread across multiple elements
      expect(screen.getByText(/100/)).toBeInTheDocument();
      expect(screen.getByText(/indexed/)).toBeInTheDocument();
      expect(screen.getByText(/500 chunks/)).toBeInTheDocument();
      expect(screen.getByText(/5 failed/)).toBeInTheDocument();
    });
  });
});
