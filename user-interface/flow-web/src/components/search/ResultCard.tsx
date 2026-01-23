import { memo } from 'react';
import type { SearchResult } from '../../types/api';
import './ResultCard.css';

interface ResultCardProps {
  /** Search result data */
  result: SearchResult;
  /** Click handler for local results */
  onClick?: () => void;
  /** Whether this result is selected/focused */
  isSelected?: boolean;
}

/**
 * Displays a single search result with source badge, title, snippet, and score.
 * Memoized to prevent unnecessary re-renders in large result lists.
 */
export const ResultCard = memo(function ResultCard({
  result,
  onClick,
  isSelected = false,
}: ResultCardProps) {
  const { source, title, snippet, score, source_id, cid } = result;

  // Format score as percentage
  const scorePercent = Math.round(score * 100);

  // Truncate snippet to ~150 chars
  const truncatedSnippet = snippet
    ? snippet.length > 150
      ? `${snippet.slice(0, 150)}...`
      : snippet
    : null;

  // Format source ID for display
  const displaySourceId = source_id
    ? source === 'network' && source_id.length > 20
      ? `${source_id.slice(0, 12)}...${source_id.slice(-6)}`
      : source_id
    : null;

  const isClickable = source === 'local' && onClick;

  return (
    <article
      className={`result-card ${isSelected ? 'result-card--selected' : ''} ${
        isClickable ? 'result-card--clickable' : ''
      }`}
      onClick={isClickable ? onClick : undefined}
      onKeyDown={
        isClickable
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onClick?.();
              }
            }
          : undefined
      }
      tabIndex={isClickable ? 0 : undefined}
      role={isClickable ? 'button' : undefined}
      aria-label={`${source === 'local' ? 'Local' : 'Network'} result: ${title || 'Untitled'}`}
    >
      <div className="result-card__header">
        <span
          className={`result-card__badge result-card__badge--${source}`}
          aria-label={source === 'local' ? 'Local result' : 'Network result'}
        >
          {source === 'local' ? 'LOCAL' : 'NETWORK'}
        </span>
        <span className="result-card__score" title={`Relevance: ${scorePercent}%`}>
          {scorePercent}%
        </span>
      </div>

      <h3 className="result-card__title">{title || 'Untitled'}</h3>

      {truncatedSnippet && (
        <p className="result-card__snippet">{truncatedSnippet}</p>
      )}

      <div className="result-card__footer">
        {displaySourceId && (
          <span className="result-card__source" title={source_id}>
            {source === 'local' ? '📁' : '🌐'} {displaySourceId}
          </span>
        )}
        <span className="result-card__cid" title={cid}>
          CID: {cid.slice(0, 12)}...
        </span>
      </div>
    </article>
  );
});
