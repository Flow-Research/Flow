import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ResultCard } from './ResultCard';
import { mockLocalResult, mockNetworkResult } from '../../test/fixtures/search';

describe('ResultCard', () => {
  const mockOnClick = vi.fn();

  beforeEach(() => {
    mockOnClick.mockClear();
  });

  describe('rendering local results', () => {
    it('renders local result with LOCAL badge', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByText('LOCAL')).toBeInTheDocument();
    });

    it('renders title', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByText('Test Document')).toBeInTheDocument();
    });

    it('renders score as percentage', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByText('92%')).toBeInTheDocument();
    });

    it('renders snippet', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByText(/This is a test document/)).toBeInTheDocument();
    });

    it('truncates long snippets', () => {
      const longSnippet = 'x'.repeat(200);
      const result = { ...mockLocalResult, snippet: longSnippet };
      render(<ResultCard result={result} />);

      const snippetEl = screen.getByText(/^x+\.\.\.$/);
      expect(snippetEl.textContent).toHaveLength(153); // 150 chars + '...'
    });

    it('renders source ID with folder icon for local', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByText(/📁.*my-space/)).toBeInTheDocument();
    });

    it('renders truncated CID', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByText(/CID: bafybeigdyrz\.\.\.$/)).toBeInTheDocument();
    });

    it('renders Untitled for results without title', () => {
      const result = { ...mockLocalResult, title: undefined };
      render(<ResultCard result={result} />);

      expect(screen.getByText('Untitled')).toBeInTheDocument();
    });
  });

  describe('rendering network results', () => {
    it('renders network result with NETWORK badge', () => {
      render(<ResultCard result={mockNetworkResult} />);

      expect(screen.getByText('NETWORK')).toBeInTheDocument();
    });

    it('renders source ID with globe icon for network', () => {
      render(<ResultCard result={mockNetworkResult} />);

      expect(screen.getByText(/🌐/)).toBeInTheDocument();
    });

    it('truncates long peer IDs', () => {
      const result = {
        ...mockNetworkResult,
        source_id: '12D3KooWxyzabcdefghijklmnopqrstuvwxyz',
      };
      render(<ResultCard result={result} />);

      // Should show first 12 and last 6 characters: 12D3KooWxyza...uvwxyz
      expect(screen.getByText(/12D3KooWxyza\.\.\.uvwxyz/)).toBeInTheDocument();
    });
  });

  describe('click behavior', () => {
    it('is clickable for local results when onClick provided', async () => {
      const user = userEvent.setup();
      render(<ResultCard result={mockLocalResult} onClick={mockOnClick} />);

      const card = screen.getByRole('button');
      await user.click(card);

      expect(mockOnClick).toHaveBeenCalledTimes(1);
    });

    it('is not clickable for network results', () => {
      render(<ResultCard result={mockNetworkResult} onClick={mockOnClick} />);

      expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });

    it('is not clickable when no onClick provided', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.queryByRole('button')).not.toBeInTheDocument();
    });

    it('responds to Enter key on clickable cards', () => {
      render(<ResultCard result={mockLocalResult} onClick={mockOnClick} />);

      const card = screen.getByRole('button');
      fireEvent.keyDown(card, { key: 'Enter' });

      expect(mockOnClick).toHaveBeenCalledTimes(1);
    });

    it('responds to Space key on clickable cards', () => {
      render(<ResultCard result={mockLocalResult} onClick={mockOnClick} />);

      const card = screen.getByRole('button');
      fireEvent.keyDown(card, { key: ' ' });

      expect(mockOnClick).toHaveBeenCalledTimes(1);
    });
  });

  describe('selected state', () => {
    it('applies selected class when isSelected is true', () => {
      render(<ResultCard result={mockLocalResult} isSelected />);

      expect(document.querySelector('.result-card--selected')).toBeInTheDocument();
    });

    it('does not apply selected class when isSelected is false', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(document.querySelector('.result-card--selected')).not.toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has accessible label for local result', () => {
      render(<ResultCard result={mockLocalResult} onClick={mockOnClick} />);

      expect(screen.getByLabelText('Local result: Test Document')).toBeInTheDocument();
    });

    it('has accessible label for network result', () => {
      render(<ResultCard result={mockNetworkResult} />);

      expect(screen.getByLabelText('Network result: Network Result')).toBeInTheDocument();
    });

    it('has tabIndex on clickable cards', () => {
      render(<ResultCard result={mockLocalResult} onClick={mockOnClick} />);

      expect(screen.getByRole('button')).toHaveAttribute('tabIndex', '0');
    });

    it('badge has aria-label for local', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByLabelText('Local result')).toBeInTheDocument();
    });

    it('badge has aria-label for network', () => {
      render(<ResultCard result={mockNetworkResult} />);

      expect(screen.getByLabelText('Network result')).toBeInTheDocument();
    });

    it('score has title with percentage', () => {
      render(<ResultCard result={mockLocalResult} />);

      expect(screen.getByTitle('Relevance: 92%')).toBeInTheDocument();
    });
  });

  describe('edge cases', () => {
    it('handles result without snippet', () => {
      const result = { ...mockLocalResult, snippet: undefined };
      render(<ResultCard result={result} />);

      expect(screen.queryByText(/This is a test/)).not.toBeInTheDocument();
    });

    it('handles result without source_id', () => {
      const result = { ...mockLocalResult, source_id: undefined };
      render(<ResultCard result={result} />);

      expect(screen.queryByText(/📁/)).not.toBeInTheDocument();
    });

    it('handles score of 0', () => {
      const result = { ...mockLocalResult, score: 0 };
      render(<ResultCard result={result} />);

      expect(screen.getByText('0%')).toBeInTheDocument();
    });

    it('handles score of 1', () => {
      const result = { ...mockLocalResult, score: 1 };
      render(<ResultCard result={result} />);

      expect(screen.getByText('100%')).toBeInTheDocument();
    });
  });
});
