import type { SpaceStatus } from '../../services/api';
import { SyncProgress } from './SyncProgress';
import './SyncStatus.css';

interface SyncStatusProps {
  /** Space status data */
  status: SpaceStatus | null;
  /** Whether status is loading */
  isLoading: boolean;
  /** Error message if any */
  error: string | null;
  /** Whether re-indexing is in progress */
  isReindexing: boolean;
  /** Callback to trigger re-indexing */
  onReindex: () => void;
  /** Callback to refresh status */
  onRefresh: () => void;
  /** Whether to show the re-index button */
  showReindex?: boolean;
}

/**
 * Displays the indexing/sync status for a space.
 * Shows progress during indexing and summary when complete.
 */
export function SyncStatus({
  status,
  isLoading,
  error,
  isReindexing,
  onReindex,
  onRefresh,
  showReindex = true,
}: SyncStatusProps) {
  if (isLoading && !status) {
    return (
      <div className="sync-status sync-status--loading">
        <div className="sync-status__skeleton" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="sync-status sync-status--error">
        <div className="sync-status__error-icon">⚠️</div>
        <div className="sync-status__error-content">
          <span className="sync-status__error-text">{error}</span>
          <button
            type="button"
            className="sync-status__retry-btn"
            onClick={onRefresh}
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (!status) {
    return null;
  }

  const isIndexing = status.indexing_in_progress || isReindexing;

  // Determine status type
  let statusType: 'syncing' | 'synced' | 'error' = 'synced';
  if (isIndexing) {
    statusType = 'syncing';
  } else if (status.last_error) {
    statusType = 'error';
  }

  // Format timestamp
  const lastIndexedText = status.last_indexed
    ? new Date(status.last_indexed).toLocaleString(undefined, {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
      })
    : 'Never';

  return (
    <div className={`sync-status sync-status--${statusType}`}>
      <div className="sync-status__header">
        <div className="sync-status__badge-group">
          <span className={`sync-status__badge sync-status__badge--${statusType}`}>
            {statusType === 'syncing' && (
              <span className="sync-status__spinner" aria-hidden="true" />
            )}
            {statusType === 'synced' && '✓'}
            {statusType === 'error' && '!'}
            <span>
              {statusType === 'syncing'
                ? 'Indexing'
                : statusType === 'error'
                ? 'Error'
                : 'Synced'}
            </span>
          </span>
        </div>

        {showReindex && !isIndexing && (
          <button
            type="button"
            className="sync-status__reindex-btn"
            onClick={onReindex}
            disabled={isReindexing}
            title="Re-index all files in this space"
          >
            🔄 Re-index
          </button>
        )}
      </div>

      {isIndexing && (
        <SyncProgress
          filesIndexed={status.files_indexed}
          chunksStored={status.chunks_stored}
          filesFailed={status.files_failed}
        />
      )}

      <div className="sync-status__stats">
        <div className="sync-status__stat">
          <span className="sync-status__stat-value">{status.files_indexed}</span>
          <span className="sync-status__stat-label">Files</span>
        </div>
        <div className="sync-status__stat">
          <span className="sync-status__stat-value">{status.chunks_stored}</span>
          <span className="sync-status__stat-label">Chunks</span>
        </div>
        <div className="sync-status__stat">
          <span className="sync-status__stat-value">{lastIndexedText}</span>
          <span className="sync-status__stat-label">Last Indexed</span>
        </div>
        {status.files_failed > 0 && (
          <div className="sync-status__stat sync-status__stat--error">
            <span className="sync-status__stat-value">{status.files_failed}</span>
            <span className="sync-status__stat-label">Failed</span>
          </div>
        )}
      </div>

      {status.last_error && (
        <details className="sync-status__error-details">
          <summary className="sync-status__error-summary">
            View last error
          </summary>
          <pre className="sync-status__error-message">{status.last_error}</pre>
        </details>
      )}
    </div>
  );
}
