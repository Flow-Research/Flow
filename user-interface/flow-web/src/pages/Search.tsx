import { useState, useCallback, useEffect } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { SearchBar, ScopeSelector, SearchResults } from '../components/search';
import { NetworkResultPreview } from '../components/preview';
import { useDistributedSearch } from '../hooks/useDistributedSearch';
import type { SearchScope, SearchResult } from '../types/api';
import './Search.css';

// Persist scope preference in localStorage
const SCOPE_STORAGE_KEY = 'flow_search_scope';

function getStoredScope(): SearchScope {
  const stored = localStorage.getItem(SCOPE_STORAGE_KEY);
  if (stored === 'local' || stored === 'network' || stored === 'all') {
    return stored;
  }
  return 'all';
}

function setStoredScope(scope: SearchScope): void {
  localStorage.setItem(SCOPE_STORAGE_KEY, scope);
}

/**
 * Global search page for distributed search across local and network sources.
 */
export function SearchPage() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  // Get initial query from URL if present
  const initialQuery = searchParams.get('q') || '';
  const initialScope = (searchParams.get('scope') as SearchScope) || getStoredScope();

  const [query, setQuery] = useState(initialQuery);
  const [scope, setScope] = useState<SearchScope>(initialScope);
  const [previewResult, setPreviewResult] = useState<SearchResult | null>(null);

  const {
    state,
    results,
    stats,
    error,
    search,
    loadMore,
    clear,
    hasMore,
  } = useDistributedSearch();

  // Execute initial search if query is in URL
  useEffect(() => {
    if (initialQuery) {
      search(initialQuery, initialScope);
    }
    // Only run on mount
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Update URL when search is performed
  const updateUrl = useCallback(
    (newQuery: string, newScope: SearchScope) => {
      if (newQuery) {
        setSearchParams({ q: newQuery, scope: newScope });
      } else {
        setSearchParams({});
      }
    },
    [setSearchParams]
  );

  const handleQueryChange = useCallback(
    (newQuery: string) => {
      setQuery(newQuery);
      search(newQuery, scope);
      updateUrl(newQuery, scope);
    },
    [scope, search, updateUrl]
  );

  const handleScopeChange = useCallback(
    (newScope: SearchScope) => {
      setScope(newScope);
      setStoredScope(newScope);
      if (query) {
        search(query, newScope);
        updateUrl(query, newScope);
      }
    },
    [query, search, updateUrl]
  );

  const handleResultClick = useCallback(
    (result: SearchResult) => {
      // Navigate to the space containing this content
      if (result.source === 'local' && result.source_id) {
        navigate(`/spaces/${result.source_id}`);
      }
    },
    [navigate]
  );

  const handleNetworkResultClick = useCallback((result: SearchResult) => {
    setPreviewResult(result);
  }, []);

  const handleClosePreview = useCallback(() => {
    setPreviewResult(null);
  }, []);

  const handleRetry = useCallback(() => {
    if (query) {
      clear();
      search(query, scope);
    }
  }, [query, scope, search, clear]);

  const isSearching = state === 'loading';
  const showEmptyState = state === 'idle' && results.length === 0;
  const showError = state === 'error';
  const showResults = results.length > 0;
  const showNoResults = state === 'success' && results.length === 0 && query;

  return (
    <div className="search-page">
      <div className="search-page__header">
        <h1 className="search-page__title">Search</h1>
        <p className="search-page__subtitle">
          Search across your local spaces and the network
        </p>
      </div>

      <div className="search-page__controls">
        <div className="search-page__search-bar">
          <SearchBar
            initialQuery={query}
            onQueryChange={handleQueryChange}
            isSearching={isSearching}
            placeholder="Search your content..."
          />
        </div>
        <ScopeSelector
          value={scope}
          onChange={handleScopeChange}
          disabled={isSearching}
        />
      </div>

      <div className="search-page__content">
        {showEmptyState && (
          <div className="search-page__empty">
            <div className="search-page__empty-icon">🔍</div>
            <h2>Start searching</h2>
            <p>Enter a query to search across your local spaces and the network.</p>
            <div className="search-page__tips">
              <h3>Search tips:</h3>
              <ul>
                <li>Use specific keywords for better results</li>
                <li>Filter by scope to search only local or network content</li>
                <li>Press <kbd>Cmd/Ctrl + K</kbd> to quickly access search</li>
              </ul>
            </div>
          </div>
        )}

        {showError && (
          <div className="search-page__error" role="alert">
            <div className="search-page__error-icon">⚠️</div>
            <h2>Search failed</h2>
            <p>{error}</p>
            <button
              type="button"
              className="search-page__retry-btn"
              onClick={handleRetry}
            >
              Try again
            </button>
          </div>
        )}

        {showNoResults && (
          <div className="search-page__no-results">
            <div className="search-page__no-results-icon">📭</div>
            <h2>No results found</h2>
            <p>
              No content matches &quot;{query}&quot; in{' '}
              {scope === 'all'
                ? 'local or network sources'
                : scope === 'local'
                ? 'local spaces'
                : 'network peers'}
              .
            </p>
            <div className="search-page__suggestions">
              <h3>Suggestions:</h3>
              <ul>
                <li>Check your spelling</li>
                <li>Try different keywords</li>
                <li>Expand your search scope to &quot;All&quot;</li>
                <li>Make sure your content is indexed</li>
              </ul>
            </div>
          </div>
        )}

        {showResults && (
          <SearchResults
            results={results}
            stats={stats}
            hasMore={hasMore}
            isLoading={isSearching}
            onLoadMore={loadMore}
            onResultClick={handleResultClick}
            onNetworkResultClick={handleNetworkResultClick}
          />
        )}
      </div>

      <NetworkResultPreview
        result={previewResult}
        isOpen={previewResult !== null}
        onClose={handleClosePreview}
      />
    </div>
  );
}
