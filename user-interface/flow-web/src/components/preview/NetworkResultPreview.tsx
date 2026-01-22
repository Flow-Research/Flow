import { useCallback } from 'react';
import type { SearchResult } from '../../types/api';
import { Modal } from '../modals/Modal';
import './NetworkResultPreview.css';

interface NetworkResultPreviewProps {
  /** The search result to preview */
  result: SearchResult | null;
  /** Whether the preview is visible */
  isOpen: boolean;
  /** Callback to close the preview */
  onClose: () => void;
}

/**
 * Preview panel/modal for network search results.
 * Shows full content details since network content can't be directly navigated.
 */
export function NetworkResultPreview({
  result,
  isOpen,
  onClose,
}: NetworkResultPreviewProps) {
  const handleCopyCid = useCallback(async () => {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(result.cid);
    } catch {
      // Fallback for older browsers
      const textArea = document.createElement('textarea');
      textArea.value = result.cid;
      document.body.appendChild(textArea);
      textArea.select();
      document.execCommand('copy');
      document.body.removeChild(textArea);
    }
  }, [result]);

  const handleCopyPeerId = useCallback(async () => {
    if (!result?.source_id) return;
    try {
      await navigator.clipboard.writeText(result.source_id);
    } catch {
      const textArea = document.createElement('textarea');
      textArea.value = result.source_id;
      document.body.appendChild(textArea);
      textArea.select();
      document.execCommand('copy');
      document.body.removeChild(textArea);
    }
  }, [result]);

  if (!result) return null;

  const scorePercent = Math.round(result.score * 100);

  return (
    <Modal isOpen={isOpen} onClose={onClose} title="Network Result" maxWidth="lg">
      <div className="network-preview">
        <div className="network-preview__header">
          <span className="network-preview__badge">NETWORK</span>
          <span className="network-preview__score">{scorePercent}% match</span>
        </div>

        <h2 className="network-preview__title">{result.title || 'Untitled'}</h2>

        {result.snippet && (
          <div className="network-preview__content">
            <h3 className="network-preview__section-title">Content Preview</h3>
            <p className="network-preview__snippet">{result.snippet}</p>
          </div>
        )}

        <div className="network-preview__metadata">
          <h3 className="network-preview__section-title">Details</h3>

          <div className="network-preview__field">
            <span className="network-preview__label">Content ID (CID)</span>
            <div className="network-preview__value-row">
              <code className="network-preview__value">{result.cid}</code>
              <button
                type="button"
                className="network-preview__copy-btn"
                onClick={handleCopyCid}
                aria-label="Copy CID"
              >
                📋
              </button>
            </div>
          </div>

          {result.source_id && (
            <div className="network-preview__field">
              <span className="network-preview__label">Publisher Peer ID</span>
              <div className="network-preview__value-row">
                <code className="network-preview__value">{result.source_id}</code>
                <button
                  type="button"
                  className="network-preview__copy-btn"
                  onClick={handleCopyPeerId}
                  aria-label="Copy Peer ID"
                >
                  📋
                </button>
              </div>
            </div>
          )}
        </div>

        <div className="network-preview__note">
          <p>
            This content is published on the Flow Network by another peer.
            To access the full content, you would need to request it from the network.
          </p>
        </div>
      </div>
    </Modal>
  );
}
