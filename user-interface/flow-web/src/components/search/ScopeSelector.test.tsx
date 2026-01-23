import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ScopeSelector } from './ScopeSelector';

describe('ScopeSelector', () => {
  const mockOnChange = vi.fn();

  beforeEach(() => {
    mockOnChange.mockClear();
  });

  describe('rendering', () => {
    it('renders all three scope options', () => {
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      expect(screen.getByRole('radio', { name: 'All' })).toBeInTheDocument();
      expect(screen.getByRole('radio', { name: 'Local' })).toBeInTheDocument();
      expect(screen.getByRole('radio', { name: 'Network' })).toBeInTheDocument();
    });

    it('marks the selected option as checked', () => {
      render(<ScopeSelector value="local" onChange={mockOnChange} />);

      expect(screen.getByRole('radio', { name: 'All' })).toHaveAttribute('aria-checked', 'false');
      expect(screen.getByRole('radio', { name: 'Local' })).toHaveAttribute('aria-checked', 'true');
      expect(screen.getByRole('radio', { name: 'Network' })).toHaveAttribute('aria-checked', 'false');
    });

    it('has correct description tooltips', () => {
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      expect(screen.getByTitle('Search local and network')).toBeInTheDocument();
      expect(screen.getByTitle('Search your spaces only')).toBeInTheDocument();
      expect(screen.getByTitle('Search network peers only')).toBeInTheDocument();
    });
  });

  describe('user interactions', () => {
    it('calls onChange when clicking an option', async () => {
      const user = userEvent.setup();
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      await user.click(screen.getByRole('radio', { name: 'Local' }));

      expect(mockOnChange).toHaveBeenCalledWith('local');
    });

    it('calls onChange with network scope', async () => {
      const user = userEvent.setup();
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      await user.click(screen.getByRole('radio', { name: 'Network' }));

      expect(mockOnChange).toHaveBeenCalledWith('network');
    });

    it('calls onChange when selecting already selected option', async () => {
      const user = userEvent.setup();
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      await user.click(screen.getByRole('radio', { name: 'All' }));

      expect(mockOnChange).toHaveBeenCalledWith('all');
    });
  });

  describe('disabled state', () => {
    it('disables all options when disabled prop is true', () => {
      render(<ScopeSelector value="all" onChange={mockOnChange} disabled />);

      expect(screen.getByRole('radio', { name: 'All' })).toBeDisabled();
      expect(screen.getByRole('radio', { name: 'Local' })).toBeDisabled();
      expect(screen.getByRole('radio', { name: 'Network' })).toBeDisabled();
    });

    it('does not call onChange when clicking disabled option', async () => {
      const user = userEvent.setup();
      render(<ScopeSelector value="all" onChange={mockOnChange} disabled />);

      await user.click(screen.getByRole('radio', { name: 'Local' }));

      expect(mockOnChange).not.toHaveBeenCalled();
    });
  });

  describe('accessibility', () => {
    it('has radiogroup role on container', () => {
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      expect(screen.getByRole('radiogroup')).toBeInTheDocument();
    });

    it('has accessible label on radiogroup', () => {
      render(<ScopeSelector value="all" onChange={mockOnChange} />);

      expect(screen.getByRole('radiogroup')).toHaveAttribute('aria-label', 'Search scope');
    });
  });

  describe('visual state', () => {
    it('applies selected class to selected option', () => {
      render(<ScopeSelector value="network" onChange={mockOnChange} />);

      const networkButton = screen.getByRole('radio', { name: 'Network' });
      expect(networkButton).toHaveClass('scope-selector__option--selected');

      const allButton = screen.getByRole('radio', { name: 'All' });
      expect(allButton).not.toHaveClass('scope-selector__option--selected');
    });
  });
});
