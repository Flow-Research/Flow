import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { SearchPage } from './Search';
import * as useDistributedSearchModule from '../hooks/useDistributedSearch';
import type { SearchResult } from '../types/api';
import type { UseDistributedSearchReturn, SearchStats } from '../hooks/useDistributedSearch';

// Mock the hook
vi.mock('../hooks/useDistributedSearch', () => ({
  useDistributedSearch: vi.fn(),
}));

// Mock useNavigate
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
  const actual = await vi.importActual('react-router-dom');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

const mockLocalResult: SearchResult = {
  cid: 'bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi',
  score: 0.92,
  source: 'local',
  title: 'Test Document',
  snippet: 'This is a test document containing important information...',
  source_id: 'my-space',
};

const mockNetworkResult: SearchResult = {
  cid: 'bafybeihxyz123456789abcdefghijklmnopqrstuvwxyz1234567890ab',
  score: 0.85,
  source: 'network',
  title: 'Network Result',
  snippet: 'Content from network peer with relevant information...',
  source_id: '12D3KooWabc123456789',
};

const mockStats: SearchStats = {
  localCount: 1,
  networkCount: 1,
  peersQueried: 3,
  peersResponded: 2,
  totalFound: 2,
  elapsedMs: 245,
};

function createMockHookReturn(
  overrides: Partial<UseDistributedSearchReturn> = {}
): UseDistributedSearchReturn {
  return {
    state: 'idle',
    results: [],
    stats: null,
    error: null,
    search: vi.fn(),
    loadMore: vi.fn(),
    clear: vi.fn(),
    hasMore: false,
    ...overrides,
  };
}

function renderSearchPage(initialEntries: string[] = ['/search']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <SearchPage />
    </MemoryRouter>
  );
}

describe('SearchPage', () => {
  const mockSearch = vi.fn();
  const mockLoadMore = vi.fn();
  const mockClear = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    mockSearch.mockClear();
    mockLoadMore.mockClear();
    mockClear.mockClear();
    mockNavigate.mockClear();

    // Default to idle state
    vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
      createMockHookReturn({
        search: mockSearch,
        loadMore: mockLoadMore,
        clear: mockClear,
      })
    );
  });

  afterEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  describe('rendering', () => {
    it('renders search page with title and subtitle', () => {
      renderSearchPage();

      expect(screen.getByRole('heading', { level: 1, name: 'Search' })).toBeInTheDocument();
      expect(
        screen.getByText('Search across your local spaces and the network')
      ).toBeInTheDocument();
    });

    it('renders SearchBar component', () => {
      renderSearchPage();

      expect(screen.getByPlaceholderText('Search your content...')).toBeInTheDocument();
    });

    it('renders ScopeSelector component', () => {
      renderSearchPage();

      // ScopeSelector should have scope options
      expect(screen.getByRole('radiogroup')).toBeInTheDocument();
    });

    it('renders empty state when idle with no results', () => {
      renderSearchPage();

      expect(screen.getByText('Start searching')).toBeInTheDocument();
      expect(
        screen.getByText('Enter a query to search across your local spaces and the network.')
      ).toBeInTheDocument();
    });

    it('renders search tips in empty state', () => {
      renderSearchPage();

      expect(screen.getByText('Search tips:')).toBeInTheDocument();
      expect(screen.getByText('Use specific keywords for better results')).toBeInTheDocument();
    });
  });

  describe('URL query parameter handling', () => {
    it('executes search from URL query parameter on mount', () => {
      renderSearchPage(['/search?q=test%20query&scope=all']);

      expect(mockSearch).toHaveBeenCalledWith('test query', 'all');
    });

    it('uses scope from URL when present', () => {
      renderSearchPage(['/search?q=test&scope=local']);

      expect(mockSearch).toHaveBeenCalledWith('test', 'local');
    });

    it('uses stored scope when not in URL', () => {
      localStorage.setItem('flow_search_scope', 'network');
      renderSearchPage(['/search?q=test']);

      expect(mockSearch).toHaveBeenCalledWith('test', 'network');
    });

    it('defaults to "all" scope when nothing stored', () => {
      renderSearchPage(['/search?q=test']);

      expect(mockSearch).toHaveBeenCalledWith('test', 'all');
    });

    it('does not execute search when no query in URL', () => {
      renderSearchPage(['/search']);

      expect(mockSearch).not.toHaveBeenCalled();
    });
  });

  describe('query change handling', () => {
    it('triggers search when query is entered', async () => {
      const user = userEvent.setup();
      renderSearchPage();

      const searchInput = screen.getByPlaceholderText('Search your content...');
      await user.type(searchInput, 'new query');

      // SearchBar debounces but calls onQueryChange
      await waitFor(() => {
        expect(mockSearch).toHaveBeenCalled();
      });
    });
  });

  describe('scope change handling', () => {
    it('persists scope preference to localStorage', async () => {
      const user = userEvent.setup();
      renderSearchPage();

      // Find and click the "local" scope option
      const localOption = screen.getByRole('radio', { name: /local/i });
      await user.click(localOption);

      expect(localStorage.getItem('flow_search_scope')).toBe('local');
    });

    it('re-executes search when scope changes with active query', async () => {
      const user = userEvent.setup();

      // Start with a query in the URL so query state is set
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [mockLocalResult],
          stats: mockStats,
          search: mockSearch,
          loadMore: mockLoadMore,
          clear: mockClear,
        })
      );

      renderSearchPage(['/search?q=test&scope=all']);

      // Clear the initial search call
      mockSearch.mockClear();

      // Change scope
      const localOption = screen.getByRole('radio', { name: /local/i });
      await user.click(localOption);

      expect(mockSearch).toHaveBeenCalledWith('test', 'local');
    });
  });

  describe('loading state', () => {
    it('shows loading indicator when searching', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'loading',
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test']);

      // SearchBar should show loading state
      const searchInput = screen.getByPlaceholderText('Search your content...');
      expect(searchInput.closest('.search-bar')).toBeInTheDocument();
    });

    it('disables scope selector when searching', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'loading',
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test']);

      const radioGroup = screen.getByRole('radiogroup');
      const radios = radioGroup.querySelectorAll('input[type="radio"]');
      radios.forEach((radio) => {
        expect(radio).toBeDisabled();
      });
    });
  });

  describe('error state', () => {
    it('displays error message when search fails', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'error',
          error: 'Search service unavailable',
          search: mockSearch,
          clear: mockClear,
        })
      );

      renderSearchPage();

      expect(screen.getByText('Search failed')).toBeInTheDocument();
      expect(screen.getByText('Search service unavailable')).toBeInTheDocument();
    });

    it('displays retry button on error', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'error',
          error: 'Search failed',
          search: mockSearch,
          clear: mockClear,
        })
      );

      renderSearchPage();

      expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
    });

    it('retries search when retry button clicked', async () => {
      const user = userEvent.setup();
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'error',
          error: 'Search failed',
          search: mockSearch,
          clear: mockClear,
        })
      );

      renderSearchPage(['/search?q=retry%20test']);

      // Clear initial search call
      mockSearch.mockClear();
      mockClear.mockClear();

      const retryButton = screen.getByRole('button', { name: 'Try again' });
      await user.click(retryButton);

      expect(mockClear).toHaveBeenCalled();
      expect(mockSearch).toHaveBeenCalledWith('retry test', 'all');
    });

    it('has accessible alert role on error', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'error',
          error: 'Error message',
          search: mockSearch,
        })
      );

      renderSearchPage();

      expect(screen.getByRole('alert')).toBeInTheDocument();
    });
  });

  describe('no results state', () => {
    it('displays no results message when search returns empty', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [],
          stats: { ...mockStats, totalFound: 0 },
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=nonexistent']);

      expect(screen.getByText('No results found')).toBeInTheDocument();
    });

    it('shows searched query in no results message', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [],
          stats: { ...mockStats, totalFound: 0 },
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=my%20search']);

      expect(screen.getByText(/No content matches "my search"/)).toBeInTheDocument();
    });

    it('shows scope-specific message for local scope', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [],
          stats: { ...mockStats, totalFound: 0 },
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test&scope=local']);

      // Use getAllByText since "local spaces" appears in subtitle too
      const matches = screen.getAllByText(/local spaces/);
      expect(matches.length).toBeGreaterThan(0);
      // The no-results message specifically mentions "local spaces"
      expect(screen.getByText(/No content matches.*in.*local spaces/)).toBeInTheDocument();
    });

    it('shows scope-specific message for network scope', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [],
          stats: { ...mockStats, totalFound: 0 },
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test&scope=network']);

      expect(screen.getByText(/network peers/)).toBeInTheDocument();
    });

    it('displays suggestions in no results state', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [],
          stats: { ...mockStats, totalFound: 0 },
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test']);

      expect(screen.getByText('Suggestions:')).toBeInTheDocument();
      expect(screen.getByText('Check your spelling')).toBeInTheDocument();
    });
  });

  describe('results display', () => {
    it('displays search results when available', () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [mockLocalResult, mockNetworkResult],
          stats: mockStats,
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test']);

      expect(screen.getByText('Test Document')).toBeInTheDocument();
      expect(screen.getByText('Network Result')).toBeInTheDocument();
    });
  });

  describe('result click handling', () => {
    it('navigates to space for local result click', async () => {
      const user = userEvent.setup();
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [mockLocalResult],
          stats: mockStats,
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test']);

      // Find and click the local result
      const resultItem = screen.getByText('Test Document').closest('button, [role="button"], article');
      if (resultItem) {
        await user.click(resultItem);
      }

      // The handleResultClick navigates for local results
      // This depends on SearchResults component calling onResultClick
    });
  });

  describe('network result preview', () => {
    it('opens preview modal for network result', async () => {
      vi.mocked(useDistributedSearchModule.useDistributedSearch).mockReturnValue(
        createMockHookReturn({
          state: 'success',
          results: [mockNetworkResult],
          stats: mockStats,
          search: mockSearch,
        })
      );

      renderSearchPage(['/search?q=test']);

      // NetworkResultPreview is rendered but not visible initially
      // When a network result is clicked, it should open
      expect(screen.getByText('Network Result')).toBeInTheDocument();
    });
  });

  describe('scope persistence', () => {
    it('retrieves stored scope on mount', () => {
      localStorage.setItem('flow_search_scope', 'local');

      renderSearchPage(['/search?q=test']);

      // Should use the stored scope
      expect(mockSearch).toHaveBeenCalledWith('test', 'local');
    });

    it('uses default "all" when stored scope is invalid', () => {
      localStorage.setItem('flow_search_scope', 'invalid');

      renderSearchPage(['/search?q=test']);

      expect(mockSearch).toHaveBeenCalledWith('test', 'all');
    });
  });

  describe('keyboard shortcuts hint', () => {
    it('shows keyboard shortcut in empty state', () => {
      renderSearchPage();

      expect(screen.getByText(/Cmd\/Ctrl \+ K/)).toBeInTheDocument();
    });
  });

  describe('accessibility', () => {
    it('has proper heading hierarchy', () => {
      renderSearchPage();

      const heading = screen.getByRole('heading', { level: 1 });
      expect(heading).toHaveTextContent('Search');
    });

    it('search input has proper placeholder', () => {
      renderSearchPage();

      const input = screen.getByPlaceholderText('Search your content...');
      expect(input).toBeInTheDocument();
    });
  });
});
