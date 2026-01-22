import { useState, useCallback, useRef, useEffect } from 'react';
import './SearchBar.css';

interface SearchBarProps {
  /** Initial query value */
  initialQuery?: string;
  /** Callback when query changes */
  onQueryChange: (query: string) => void;
  /** Whether search is in progress */
  isSearching?: boolean;
  /** Placeholder text */
  placeholder?: string;
}

/**
 * Search input component with clear button and loading state.
 */
export function SearchBar({
  initialQuery = '',
  onQueryChange,
  isSearching = false,
  placeholder = 'Search across your content...',
}: SearchBarProps) {
  const [value, setValue] = useState(initialQuery);
  const inputRef = useRef<HTMLInputElement>(null);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Update internal value when initialQuery changes
  useEffect(() => {
    setValue(initialQuery);
  }, [initialQuery]);

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const newValue = e.target.value;
      setValue(newValue);
      onQueryChange(newValue);
    },
    [onQueryChange]
  );

  const handleClear = useCallback(() => {
    setValue('');
    onQueryChange('');
    inputRef.current?.focus();
  }, [onQueryChange]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Escape') {
        handleClear();
      }
    },
    [handleClear]
  );

  return (
    <div className="search-bar">
      <div className="search-bar__icon">
        {isSearching ? (
          <span className="search-bar__spinner" aria-hidden="true" />
        ) : (
          <svg
            className="search-bar__search-icon"
            width="20"
            height="20"
            viewBox="0 0 20 20"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M17.5 17.5L13.875 13.875M15.8333 9.16667C15.8333 12.8486 12.8486 15.8333 9.16667 15.8333C5.48477 15.8333 2.5 12.8486 2.5 9.16667C2.5 5.48477 5.48477 2.5 9.16667 2.5C12.8486 2.5 15.8333 5.48477 15.8333 9.16667Z"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </div>
      <input
        ref={inputRef}
        type="text"
        className="search-bar__input"
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        aria-label="Search"
        maxLength={1000}
      />
      {value && !isSearching && (
        <button
          type="button"
          className="search-bar__clear"
          onClick={handleClear}
          aria-label="Clear search"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path
              d="M12 4L4 12M4 4L12 12"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          </svg>
        </button>
      )}
    </div>
  );
}
