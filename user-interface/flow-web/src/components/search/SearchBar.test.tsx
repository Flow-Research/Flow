import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { SearchBar } from './SearchBar';

describe('SearchBar', () => {
  const mockOnQueryChange = vi.fn();

  beforeEach(() => {
    mockOnQueryChange.mockClear();
  });

  describe('rendering', () => {
    it('renders with default placeholder', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      expect(screen.getByPlaceholderText('Search across your content...')).toBeInTheDocument();
    });

    it('renders with custom placeholder', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} placeholder="Search here..." />);

      expect(screen.getByPlaceholderText('Search here...')).toBeInTheDocument();
    });

    it('renders with initial query', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="test" />);

      expect(screen.getByRole('textbox')).toHaveValue('test');
    });

    it('shows search icon when not searching', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      expect(screen.getByLabelText('Search')).toBeInTheDocument();
      expect(document.querySelector('.search-bar__search-icon')).toBeInTheDocument();
    });

    it('shows spinner when searching', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} isSearching />);

      expect(document.querySelector('.search-bar__spinner')).toBeInTheDocument();
      expect(document.querySelector('.search-bar__search-icon')).not.toBeInTheDocument();
    });

    it('focuses input on mount', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      expect(screen.getByRole('textbox')).toHaveFocus();
    });
  });

  describe('user interactions', () => {
    it('calls onQueryChange when user types', async () => {
      const user = userEvent.setup();
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      await user.type(screen.getByRole('textbox'), 'hello');

      expect(mockOnQueryChange).toHaveBeenCalledWith('h');
      expect(mockOnQueryChange).toHaveBeenCalledWith('he');
      expect(mockOnQueryChange).toHaveBeenCalledWith('hel');
      expect(mockOnQueryChange).toHaveBeenCalledWith('hell');
      expect(mockOnQueryChange).toHaveBeenCalledWith('hello');
    });

    it('shows clear button when input has value', async () => {
      const user = userEvent.setup();
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      expect(screen.queryByLabelText('Clear search')).not.toBeInTheDocument();

      await user.type(screen.getByRole('textbox'), 'test');

      expect(screen.getByLabelText('Clear search')).toBeInTheDocument();
    });

    it('clears input when clear button is clicked', async () => {
      const user = userEvent.setup();
      render(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="test" />);

      await user.click(screen.getByLabelText('Clear search'));

      expect(screen.getByRole('textbox')).toHaveValue('');
      expect(mockOnQueryChange).toHaveBeenCalledWith('');
    });

    it('focuses input after clearing', async () => {
      const user = userEvent.setup();
      render(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="test" />);

      await user.click(screen.getByLabelText('Clear search'));

      expect(screen.getByRole('textbox')).toHaveFocus();
    });

    it('clears input when Escape is pressed', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="test" />);

      fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });

      expect(screen.getByRole('textbox')).toHaveValue('');
      expect(mockOnQueryChange).toHaveBeenCalledWith('');
    });

    it('hides clear button while searching', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="test" isSearching />);

      expect(screen.queryByLabelText('Clear search')).not.toBeInTheDocument();
    });
  });

  describe('controlled updates', () => {
    it('updates value when initialQuery changes', () => {
      const { rerender } = render(
        <SearchBar onQueryChange={mockOnQueryChange} initialQuery="initial" />
      );

      expect(screen.getByRole('textbox')).toHaveValue('initial');

      rerender(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="updated" />);

      expect(screen.getByRole('textbox')).toHaveValue('updated');
    });
  });

  describe('accessibility', () => {
    it('has accessible label', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      expect(screen.getByLabelText('Search')).toBeInTheDocument();
    });

    it('has max length constraint', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} />);

      expect(screen.getByRole('textbox')).toHaveAttribute('maxLength', '1000');
    });

    it('clear button has accessible label', () => {
      render(<SearchBar onQueryChange={mockOnQueryChange} initialQuery="test" />);

      expect(screen.getByLabelText('Clear search')).toBeInTheDocument();
    });
  });
});
