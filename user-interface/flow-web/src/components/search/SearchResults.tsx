import type { SearchResult } from '../../types/api';
import type { SearchStats } from '../../hooks/useDistributedSearch';
import { ResultCard } from './ResultCard';
import './SearchResults.css';

interface SearchResultsProps {
  /** Search results to display */
  results: SearchResult[];
  /** Search statistics */
  stats: SearchStats | null;
  /** Whether more results are available */
  hasMore: boolean;
  /** Whether currently loading more */
  isLoading: boolean;
  /** Callback to load more results */
  onLoadMore: () => void;
  /** Callback when a local result is clicked */
  onResultClick?: (result: SearchResult) => void;
  /** Callback when a network result is clicked (opens preview) */
  onNetworkResultClick?: (result: SearchResult) => void;
}

/**
 * Renders a list of search results with stats and pagination.
 */
export function SearchResults({
  results,
  stats,
  hasMore,
  isLoading,
  onLoadMore,
  onResultClick,
  onNetworkResultClick,
}: SearchResultsProps) {
  if (results.length === 0 && !isLoading) {
    return null;
  }

  const handleResultClick = (result: SearchResult) => {
    if (result.source === 'local' && onResultClick) {
      onResultClick(result);
    } else if (result.source === 'network' && onNetworkResultClick) {
      onNetworkResultClick(result);
    }
  };

  return (
    <div className="search-results">
      {stats && (
        <div className="search-results__stats" role="status" aria-live="polite">
          <span className="search-results__count">
            {stats.totalFound} result{stats.totalFound !== 1 ? 's' : ''} found
          </span>
          <span className="search-results__breakdown">
            {stats.localCount > 0 && (
              <span className="search-results__local-count">
                {stats.localCount} local
              </span>
            )}
            {stats.localCount > 0 && stats.networkCount > 0 && (
              <span className="search-results__separator">·</span>
            )}
            {stats.networkCount > 0 && (
              <span className="search-results__network-count">
                {stats.networkCount} network
              </span>
            )}
          </span>
          <span className="search-results__time">
            {stats.elapsedMs}ms
            {stats.peersQueried > 0 && (
              <span className="search-results__peers">
                {' '}
                · {stats.peersResponded}/{stats.peersQueried} peers
              </span>
            )}
          </span>
        </div>
      )}

      <div className="search-results__list" role="list">
        {results.map((result, index) => (
          <div key={`${result.cid}-${index}`} role="listitem">
            <ResultCard
              result={result}
              onClick={() => handleResultClick(result)}
            />
          </div>
        ))}
      </div>

      {hasMore && (
        <div className="search-results__load-more">
          <button
            type="button"
            className="search-results__load-more-btn"
            onClick={onLoadMore}
            disabled={isLoading}
          >
            {isLoading ? 'Loading...' : 'Load more results'}
          </button>
        </div>
      )}
    </div>
  );
}
