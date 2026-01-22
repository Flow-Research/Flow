import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useKeyboardShortcut } from './useKeyboardShortcut';

describe('useKeyboardShortcut', () => {
  const mockOnTrigger = vi.fn();

  beforeEach(() => {
    mockOnTrigger.mockClear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('basic key handling', () => {
    it('triggers callback when key is pressed', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'a' });
      document.dispatchEvent(event);

      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });

    it('does not trigger for different key', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'b' });
      document.dispatchEvent(event);

      expect(mockOnTrigger).not.toHaveBeenCalled();
    });

    it('handles case-insensitive key matching', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'K',
          onTrigger: mockOnTrigger,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'k' });
      document.dispatchEvent(event);

      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });

    it('handles Escape key', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'Escape',
          onTrigger: mockOnTrigger,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'Escape' });
      document.dispatchEvent(event);

      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });
  });

  describe('modifier keys', () => {
    it('requires Ctrl key on Windows/Linux when withModifier is true', () => {
      // Mock non-Mac platform
      vi.stubGlobal('navigator', { platform: 'Win32' });

      renderHook(() =>
        useKeyboardShortcut({
          key: 'k',
          withModifier: true,
          onTrigger: mockOnTrigger,
        })
      );

      // Without modifier - should not trigger
      const eventWithoutModifier = new KeyboardEvent('keydown', { key: 'k' });
      document.dispatchEvent(eventWithoutModifier);
      expect(mockOnTrigger).not.toHaveBeenCalled();

      // With Ctrl - should trigger
      const eventWithCtrl = new KeyboardEvent('keydown', {
        key: 'k',
        ctrlKey: true,
      });
      document.dispatchEvent(eventWithCtrl);
      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });

    it('requires Meta key on Mac when withModifier is true', () => {
      // Mock Mac platform
      vi.stubGlobal('navigator', { platform: 'MacIntel' });

      renderHook(() =>
        useKeyboardShortcut({
          key: 'k',
          withModifier: true,
          onTrigger: mockOnTrigger,
        })
      );

      // Without modifier - should not trigger
      const eventWithoutModifier = new KeyboardEvent('keydown', { key: 'k' });
      document.dispatchEvent(eventWithoutModifier);
      expect(mockOnTrigger).not.toHaveBeenCalled();

      // With Meta - should trigger
      const eventWithMeta = new KeyboardEvent('keydown', {
        key: 'k',
        metaKey: true,
      });
      document.dispatchEvent(eventWithMeta);
      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });
  });

  describe('enabled state', () => {
    it('does not trigger when disabled', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
          enabled: false,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'a' });
      document.dispatchEvent(event);

      expect(mockOnTrigger).not.toHaveBeenCalled();
    });

    it('triggers when enabled is true (default)', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'a' });
      document.dispatchEvent(event);

      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });

    it('responds to enabled state changes', () => {
      const { rerender } = renderHook(
        ({ enabled }) =>
          useKeyboardShortcut({
            key: 'a',
            onTrigger: mockOnTrigger,
            enabled,
          }),
        { initialProps: { enabled: false } }
      );

      const event = new KeyboardEvent('keydown', { key: 'a' });
      document.dispatchEvent(event);
      expect(mockOnTrigger).not.toHaveBeenCalled();

      rerender({ enabled: true });
      document.dispatchEvent(event);
      expect(mockOnTrigger).toHaveBeenCalledTimes(1);
    });
  });

  describe('input/textarea handling', () => {
    it('does not trigger when target is input', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      const input = document.createElement('input');
      document.body.appendChild(input);

      const event = new KeyboardEvent('keydown', { key: 'a', bubbles: true });
      Object.defineProperty(event, 'target', { value: input });
      document.dispatchEvent(event);

      expect(mockOnTrigger).not.toHaveBeenCalled();

      document.body.removeChild(input);
    });

    it('allows Escape key in input', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'Escape',
          onTrigger: mockOnTrigger,
        })
      );

      const input = document.createElement('input');
      document.body.appendChild(input);

      const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true });
      Object.defineProperty(event, 'target', { value: input });
      document.dispatchEvent(event);

      expect(mockOnTrigger).toHaveBeenCalledTimes(1);

      document.body.removeChild(input);
    });

    it('does not trigger when target is textarea', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      const textarea = document.createElement('textarea');
      document.body.appendChild(textarea);

      const event = new KeyboardEvent('keydown', { key: 'a', bubbles: true });
      Object.defineProperty(event, 'target', { value: textarea });
      document.dispatchEvent(event);

      expect(mockOnTrigger).not.toHaveBeenCalled();

      document.body.removeChild(textarea);
    });
  });

  describe('event prevention', () => {
    it('prevents default when shortcut is triggered', () => {
      renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      const event = new KeyboardEvent('keydown', { key: 'a', cancelable: true });
      const preventDefaultSpy = vi.spyOn(event, 'preventDefault');

      document.dispatchEvent(event);

      expect(preventDefaultSpy).toHaveBeenCalled();
    });
  });

  describe('cleanup', () => {
    it('removes event listener on unmount', () => {
      const removeEventListenerSpy = vi.spyOn(document, 'removeEventListener');

      const { unmount } = renderHook(() =>
        useKeyboardShortcut({
          key: 'a',
          onTrigger: mockOnTrigger,
        })
      );

      unmount();

      expect(removeEventListenerSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
    });
  });
});
