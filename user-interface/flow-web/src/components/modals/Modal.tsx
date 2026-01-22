import { useEffect, useCallback, useRef } from 'react';
import './Modal.css';

interface ModalProps {
  /** Whether the modal is open */
  isOpen: boolean;
  /** Callback to close the modal */
  onClose: () => void;
  /** Modal title */
  title: string;
  /** Modal content */
  children: React.ReactNode;
  /** Maximum width of the modal */
  maxWidth?: 'sm' | 'md' | 'lg';
}

/**
 * Base modal component with focus trap and keyboard handling.
 */
export function Modal({
  isOpen,
  onClose,
  title,
  children,
  maxWidth = 'md',
}: ModalProps) {
  const modalRef = useRef<HTMLDivElement>(null);
  const previousActiveElement = useRef<HTMLElement | null>(null);

  // Handle escape key
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    },
    [onClose]
  );

  // Focus management
  useEffect(() => {
    if (isOpen) {
      // Store currently focused element
      previousActiveElement.current = document.activeElement as HTMLElement;

      // Add escape listener
      document.addEventListener('keydown', handleKeyDown);

      // Prevent body scroll
      document.body.style.overflow = 'hidden';

      // Focus the modal
      setTimeout(() => {
        modalRef.current?.focus();
      }, 0);

      return () => {
        document.removeEventListener('keydown', handleKeyDown);
        document.body.style.overflow = '';

        // Restore focus
        previousActiveElement.current?.focus();
      };
    }
  }, [isOpen, handleKeyDown]);

  if (!isOpen) return null;

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  return (
    <div
      className="modal-backdrop"
      onClick={handleBackdropClick}
      role="presentation"
    >
      <div
        ref={modalRef}
        className={`modal modal--${maxWidth}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="modal-title"
        tabIndex={-1}
      >
        <div className="modal__header">
          <h2 id="modal-title" className="modal__title">
            {title}
          </h2>
          <button
            type="button"
            className="modal__close"
            onClick={onClose}
            aria-label="Close modal"
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M15 5L5 15M5 5L15 15"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
            </svg>
          </button>
        </div>
        <div className="modal__content">{children}</div>
      </div>
    </div>
  );
}
