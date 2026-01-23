import { useEffect, useCallback } from 'react';

interface KeyboardShortcutOptions {
  /** The key to listen for (e.g., 'k', 'Escape') */
  key: string;
  /** Whether to require Cmd (Mac) or Ctrl (Windows/Linux) */
  withModifier?: boolean;
  /** Callback when shortcut is triggered */
  onTrigger: () => void;
  /** Whether the shortcut is enabled */
  enabled?: boolean;
}

/**
 * Custom hook for handling keyboard shortcuts.
 *
 * @example
 * ```tsx
 * // Cmd/Ctrl + K to focus search
 * useKeyboardShortcut({
 *   key: 'k',
 *   withModifier: true,
 *   onTrigger: () => navigate('/search'),
 * });
 *
 * // Escape to close modal
 * useKeyboardShortcut({
 *   key: 'Escape',
 *   onTrigger: () => setIsOpen(false),
 *   enabled: isOpen,
 * });
 * ```
 */
export function useKeyboardShortcut({
  key,
  withModifier = false,
  onTrigger,
  enabled = true,
}: KeyboardShortcutOptions): void {
  const handleKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (!enabled) return;

      // Check if we should respond to this key
      const targetKey = key.toLowerCase();
      const pressedKey = event.key.toLowerCase();

      if (pressedKey !== targetKey) return;

      // Check modifier key if required
      if (withModifier) {
        // Use metaKey for Mac, ctrlKey for Windows/Linux
        const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
        const hasModifier = isMac ? event.metaKey : event.ctrlKey;

        if (!hasModifier) return;
      }

      // Don't trigger if user is typing in an input/textarea
      const target = event.target as HTMLElement;
      if (
        target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable
      ) {
        // Allow Escape to work even in inputs
        if (key.toLowerCase() !== 'escape') {
          return;
        }
      }

      event.preventDefault();
      onTrigger();
    },
    [key, withModifier, onTrigger, enabled]
  );

  useEffect(() => {
    if (!enabled) return;

    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [handleKeyDown, enabled]);
}
