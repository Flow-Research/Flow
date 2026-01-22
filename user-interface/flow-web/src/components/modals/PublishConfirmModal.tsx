import { useState } from 'react';
import { Modal } from './Modal';
import './PublishConfirmModal.css';

interface PublishConfirmModalProps {
  /** Whether the modal is open */
  isOpen: boolean;
  /** Callback to close the modal */
  onClose: () => void;
  /** Space name/key being published */
  spaceName: string;
  /** Whether this is for unpublishing */
  isUnpublish?: boolean;
  /** Callback when user confirms */
  onConfirm: () => Promise<void>;
}

/**
 * Confirmation modal for publishing/unpublishing a space.
 * Explains privacy implications and requires explicit confirmation.
 */
export function PublishConfirmModal({
  isOpen,
  onClose,
  spaceName,
  isUnpublish = false,
  onConfirm,
}: PublishConfirmModalProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleConfirm = async () => {
    setIsLoading(true);
    setError(null);

    try {
      await onConfirm();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Operation failed');
    } finally {
      setIsLoading(false);
    }
  };

  const title = isUnpublish ? 'Unpublish Space' : 'Publish Space';

  return (
    <Modal isOpen={isOpen} onClose={onClose} title={title} maxWidth="md">
      <div className="publish-confirm">
        {isUnpublish ? (
          <>
            <p className="publish-confirm__description">
              Are you sure you want to unpublish <strong>{spaceName}</strong>?
            </p>
            <div className="publish-confirm__info">
              <h3>What happens when you unpublish:</h3>
              <ul>
                <li>Your content will no longer appear in network search results</li>
                <li>Peers who previously accessed your content may still have cached copies</li>
                <li>Changes may take up to 5 minutes to propagate across the network</li>
                <li>You can republish at any time</li>
              </ul>
            </div>
          </>
        ) : (
          <>
            <p className="publish-confirm__description">
              You are about to publish <strong>{spaceName}</strong> to the Flow Network.
            </p>
            <div className="publish-confirm__info publish-confirm__info--warning">
              <h3>⚠️ Privacy Notice</h3>
              <ul>
                <li>Your content will be <strong>discoverable</strong> by anyone on the network</li>
                <li>Other users can search and find your published content</li>
                <li>Content is stored in a distributed network (IPFS)</li>
                <li>Publishing is not immediate - indexing may take a few minutes</li>
              </ul>
            </div>
            <div className="publish-confirm__info">
              <h3>What gets shared:</h3>
              <ul>
                <li>Document titles and content snippets</li>
                <li>Semantic embeddings for search</li>
                <li>Your node&apos;s peer ID as the publisher</li>
              </ul>
            </div>
          </>
        )}

        {error && (
          <div className="publish-confirm__error" role="alert">
            {error}
          </div>
        )}

        <div className="publish-confirm__actions">
          <button
            type="button"
            className="publish-confirm__btn publish-confirm__btn--cancel"
            onClick={onClose}
            disabled={isLoading}
          >
            Cancel
          </button>
          <button
            type="button"
            className={`publish-confirm__btn publish-confirm__btn--confirm ${
              isUnpublish ? 'publish-confirm__btn--danger' : ''
            }`}
            onClick={handleConfirm}
            disabled={isLoading}
          >
            {isLoading
              ? isUnpublish
                ? 'Unpublishing...'
                : 'Publishing...'
              : isUnpublish
              ? 'Unpublish'
              : 'Publish to Network'}
          </button>
        </div>
      </div>
    </Modal>
  );
}
