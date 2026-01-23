import { useState, useCallback } from 'react';
import { api } from '../../services/api';
import { PublishConfirmModal } from '../modals/PublishConfirmModal';
import './PublishButton.css';

interface PublishButtonProps {
  /** Space key */
  spaceKey: string;
  /** Space display name */
  spaceName: string;
  /** Whether space is currently published */
  isPublished: boolean;
  /** Published timestamp */
  publishedAt?: string | null;
  /** Callback after publish/unpublish */
  onStatusChange: (isPublished: boolean, publishedAt?: string) => void;
}

/**
 * Button component for publishing/unpublishing a space to the network.
 * Shows current status and opens confirmation modal on click.
 */
export function PublishButton({
  spaceKey,
  spaceName,
  isPublished,
  publishedAt,
  onStatusChange,
}: PublishButtonProps) {
  const [showModal, setShowModal] = useState(false);
  const [showUnpublishModal, setShowUnpublishModal] = useState(false);
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);

  const handlePublishClick = useCallback(() => {
    if (isPublished) {
      // Toggle dropdown for published state
      setIsDropdownOpen((prev) => !prev);
    } else {
      // Open publish confirmation modal
      setShowModal(true);
    }
  }, [isPublished]);

  const handleUnpublishClick = useCallback(() => {
    setIsDropdownOpen(false);
    setShowUnpublishModal(true);
  }, []);

  const handlePublish = useCallback(async () => {
    const response = await api.publish.publish(spaceKey);
    if (response.success) {
      onStatusChange(true, response.published_at);
    } else {
      throw new Error(response.error || 'Failed to publish');
    }
  }, [spaceKey, onStatusChange]);

  const handleUnpublish = useCallback(async () => {
    const response = await api.publish.unpublish(spaceKey);
    if (response.success) {
      onStatusChange(false);
    } else {
      throw new Error('Failed to unpublish');
    }
  }, [spaceKey, onStatusChange]);

  const closeDropdown = useCallback(() => {
    setIsDropdownOpen(false);
  }, []);

  // Format published date
  const formattedDate = publishedAt
    ? new Date(publishedAt).toLocaleDateString(undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      })
    : null;

  return (
    <>
      <div className="publish-button-container">
        <button
          type="button"
          className={`publish-button ${
            isPublished ? 'publish-button--published' : ''
          }`}
          onClick={handlePublishClick}
          aria-pressed={isPublished}
          aria-haspopup={isPublished ? 'menu' : undefined}
          aria-expanded={isPublished ? isDropdownOpen : undefined}
        >
          {isPublished ? (
            <>
              <span className="publish-button__status-dot" aria-hidden="true" />
              <span>Published</span>
              <svg
                className={`publish-button__chevron ${
                  isDropdownOpen ? 'publish-button__chevron--open' : ''
                }`}
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M4 6L8 10L12 6"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </>
          ) : (
            <>
              <svg
                className="publish-button__icon"
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M8 3V13M3 8H13"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
              <span>Publish</span>
            </>
          )}
        </button>

        {isDropdownOpen && (
          <>
            <div
              className="publish-button__backdrop"
              onClick={closeDropdown}
              aria-hidden="true"
            />
            <div className="publish-button__dropdown" role="menu">
              {formattedDate && (
                <div className="publish-button__dropdown-info">
                  Published {formattedDate}
                </div>
              )}
              <button
                type="button"
                className="publish-button__dropdown-item publish-button__dropdown-item--danger"
                onClick={handleUnpublishClick}
                role="menuitem"
              >
                Unpublish
              </button>
            </div>
          </>
        )}
      </div>

      <PublishConfirmModal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
        spaceName={spaceName || spaceKey}
        onConfirm={handlePublish}
      />

      <PublishConfirmModal
        isOpen={showUnpublishModal}
        onClose={() => setShowUnpublishModal(false)}
        spaceName={spaceName || spaceKey}
        isUnpublish
        onConfirm={handleUnpublish}
      />
    </>
  );
}
