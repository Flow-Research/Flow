import './SyncProgress.css';

interface SyncProgressProps {
  /** Number of files indexed so far */
  filesIndexed: number;
  /** Number of chunks stored */
  chunksStored: number;
  /** Number of files that failed */
  filesFailed: number;
}

/**
 * Progress indicator shown during active indexing.
 * Displays a pulsing animation and current counts.
 */
export function SyncProgress({
  filesIndexed,
  chunksStored,
  filesFailed,
}: SyncProgressProps) {
  return (
    <div className="sync-progress" role="status" aria-live="polite">
      <div className="sync-progress__bar">
        <div className="sync-progress__bar-fill" />
      </div>
      <div className="sync-progress__text">
        <span>Processing files...</span>
        <span className="sync-progress__counts">
          {filesIndexed} indexed
          {chunksStored > 0 && ` · ${chunksStored} chunks`}
          {filesFailed > 0 && (
            <span className="sync-progress__failed"> · {filesFailed} failed</span>
          )}
        </span>
      </div>
    </div>
  );
}
