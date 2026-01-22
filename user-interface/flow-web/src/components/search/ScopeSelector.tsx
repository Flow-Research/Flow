import type { SearchScope } from '../../types/api';
import './ScopeSelector.css';

interface ScopeSelectorProps {
  /** Current selected scope */
  value: SearchScope;
  /** Callback when scope changes */
  onChange: (scope: SearchScope) => void;
  /** Disabled state */
  disabled?: boolean;
}

interface ScopeOption {
  value: SearchScope;
  label: string;
  description: string;
}

const SCOPE_OPTIONS: ScopeOption[] = [
  {
    value: 'all',
    label: 'All',
    description: 'Search local and network',
  },
  {
    value: 'local',
    label: 'Local',
    description: 'Search your spaces only',
  },
  {
    value: 'network',
    label: 'Network',
    description: 'Search network peers only',
  },
];

/**
 * Scope selector for distributed search.
 * Allows choosing between local, network, or all sources.
 */
export function ScopeSelector({
  value,
  onChange,
  disabled = false,
}: ScopeSelectorProps) {
  return (
    <div className="scope-selector" role="radiogroup" aria-label="Search scope">
      {SCOPE_OPTIONS.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={value === option.value}
          className={`scope-selector__option ${
            value === option.value ? 'scope-selector__option--selected' : ''
          }`}
          onClick={() => onChange(option.value)}
          disabled={disabled}
          title={option.description}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
