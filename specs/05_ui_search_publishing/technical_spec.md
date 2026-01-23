# Technical Specification: UI for Search, Publishing & Network Features

**Version:** 1.0
**Date:** 2026-01-22
**Status:** Draft
**Author:** Claude (AI Assistant)

---

## 1. Overview

### 1.1 Purpose

This document provides the technical blueprint for implementing four UI features in Flow Network's React frontend:

1. **Distributed Search UI** - Global search page with scope selection
2. **Content Publishing UI** - Publish/unpublish spaces with confirmation flow
3. **Sync Status UI** - Real-time indexing status and progress
4. **Network Status UI** - Connection indicator and peer information

### 1.2 Scope

| In Scope | Out of Scope |
|----------|--------------|
| New `/search` page and components | Backend API changes |
| Space page enhancements | Real-time WebSocket connections |
| Settings page network section | Search autocomplete/suggestions |
| Header network indicator | Advanced search filters |
| API service extensions | Mobile-responsive design |

### 1.3 Technical Context

**Existing Stack:**
- React 18.2+ with TypeScript
- Vite for build/dev server
- React Router v6 for routing
- Vitest for testing
- CSS modules (no UI framework)
- Centralized `api.ts` service layer

**Existing Patterns:**
- Pages in `src/pages/` directory
- Reusable components in `src/components/`
- Context for global state (`AuthContext`)
- Test files co-located with source (`.test.tsx`)

---

## 2. System Architecture

### 2.1 Component Architecture

```
src/
├── components/
│   ├── layout/
│   │   ├── Layout.tsx           # MODIFY: Add search link, network indicator
│   │   └── Layout.css
│   ├── search/                  # NEW
│   │   ├── SearchBar.tsx
│   │   ├── SearchBar.css
│   │   ├── ScopeSelector.tsx
│   │   ├── ScopeSelector.css
│   │   ├── SearchResults.tsx
│   │   ├── SearchResults.css
│   │   ├── ResultCard.tsx
│   │   └── ResultCard.css
│   ├── space/                   # NEW
│   │   ├── PublishButton.tsx
│   │   ├── PublishButton.css
│   │   ├── SyncStatus.tsx
│   │   ├── SyncStatus.css
│   │   └── SyncProgress.tsx
│   ├── network/                 # NEW
│   │   ├── NetworkIndicator.tsx
│   │   ├── NetworkIndicator.css
│   │   ├── NetworkDetails.tsx
│   │   └── NetworkDetails.css
│   ├── modals/                  # NEW
│   │   ├── PublishConfirmModal.tsx
│   │   ├── PublishConfirmModal.css
│   │   ├── ErrorDetailsModal.tsx
│   │   └── Modal.tsx            # Base modal component
│   └── preview/                 # NEW
│       ├── NetworkResultPreview.tsx
│       └── NetworkResultPreview.css
├── pages/
│   ├── Search.tsx               # NEW
│   ├── Search.css               # NEW
│   ├── Search.test.tsx          # NEW
│   ├── Space.tsx                # MODIFY: Add publish/sync sections
│   └── Settings.tsx             # MODIFY: Add network section
├── services/
│   └── api.ts                   # MODIFY: Add new API methods
├── hooks/                       # NEW
│   ├── useDistributedSearch.ts
│   ├── useSpaceStatus.ts
│   ├── useNetworkStatus.ts
│   └── useKeyboardShortcut.ts
├── contexts/
│   └── NetworkContext.tsx       # NEW: Global network state
└── types/
    └── api.ts                   # NEW: Shared API types
```

### 2.2 State Management Strategy

| State Type | Location | Rationale |
|------------|----------|-----------|
| Auth state | `AuthContext` | Already exists, global |
| Network status | `NetworkContext` | Global, shown in header |
| Search state | Local (Search page) | Page-specific, no sharing needed |
| Space status | Local (Space page) | Page-specific, fetched per-space |
| Modal state | Local (parent component) | Simple open/close |

### 2.3 Data Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                         React App                                 │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐  │
│  │ AuthContext │    │NetworkContext│   │ Component State     │  │
│  │ (global)    │    │ (global)    │    │ (local)             │  │
│  └──────┬──────┘    └──────┬──────┘    └──────────┬──────────┘  │
│         │                  │                       │             │
│         └──────────────────┼───────────────────────┘             │
│                            │                                      │
│                     ┌──────▼──────┐                              │
│                     │  api.ts     │                              │
│                     │  service    │                              │
│                     └──────┬──────┘                              │
│                            │                                      │
└────────────────────────────┼──────────────────────────────────────┘
                             │ HTTP
                             ▼
                    ┌────────────────┐
                    │  Backend API   │
                    │  :8080/api/v1  │
                    └────────────────┘
```

---

## 3. Data Architecture

### 3.1 API Types (`src/types/api.ts`)

```typescript
// Distributed Search
export type SearchScope = 'local' | 'network' | 'all';

export interface DistributedSearchRequest {
  query: string;
  scope: SearchScope;
  limit?: number;
  offset?: number;  // For pagination: skip first N results
}

export interface SearchResult {
  cid: string;
  score: number;
  source: 'local' | 'network';
  title?: string;
  snippet?: string;
  source_id?: string;
}

export interface DistributedSearchResponse {
  success: boolean;
  query: string;
  scope: string;
  results: SearchResult[];
  local_count: number;
  network_count: number;
  peers_queried: number;
  peers_responded: number;
  total_found: number;
  elapsed_ms: number;
}

// Space Status (already partially exists)
export interface SpaceStatus {
  indexing_in_progress: boolean;
  last_indexed: string | null;
  files_indexed: number;
  chunks_stored: number;
  files_failed: number;
  last_error: string | null;
}

// Publishing
export interface PublishResponse {
  success: boolean;
  cid?: string;
  published_at?: string;
  error?: string;
}

export interface Space {
  id: number;
  key: string;
  name: string | null;
  location: string;
  time_created: string;
  is_published?: boolean;
  published_at?: string;
}

// Network Status
export interface PeerInfo {
  id: string;
  connected_at: string;
}

export interface NetworkStatus {
  connected: boolean;
  peer_count: number;
  peers: PeerInfo[];
}
```

### 3.2 Local Storage Schema

| Key | Type | Purpose |
|-----|------|---------|
| `flow_search_scope` | `SearchScope` | Persist user's preferred search scope |
| `flow_search_history` | `string[]` | [FUTURE] Recent search queries |

---

## 4. API Specification

### 4.1 API Service Extensions (`src/services/api.ts`)

```typescript
export const api = {
  // ... existing methods

  search: {
    distributed: (request: DistributedSearchRequest) =>
      request<DistributedSearchResponse>('/search/distributed', {
        method: 'POST',
        body: JSON.stringify(request),
      }),

    health: () =>
      request<{ status: string; cache: { entries: number; hits: number; misses: number } }>(
        '/search/distributed/health'
      ),
  },

  spaces: {
    // ... existing methods

    publish: (key: string) =>
      request<PublishResponse>(`/spaces/${key}/publish`, { method: 'POST' }),

    unpublish: (key: string) =>
      request<{ success: boolean }>(`/spaces/${key}/publish`, { method: 'DELETE' }),
  },

  network: {
    status: () => request<NetworkStatus>('/network/status'),
  },
};
```

### 4.2 API Endpoints Summary

| Endpoint | Method | Component | Purpose |
|----------|--------|-----------|---------|
| `/search/distributed` | POST | SearchPage | Execute distributed search |
| `/search/distributed/health` | GET | [Future] | Search system health |
| `/spaces/{key}/status` | GET | SpacePage | Get indexing status |
| `/spaces/{key}/reindex` | POST | SpacePage | Trigger re-indexing |
| `/spaces/{key}/publish` | POST | SpacePage | Publish space |
| `/spaces/{key}/publish` | DELETE | SpacePage | Unpublish space |
| `/network/status` | GET | NetworkIndicator | Get network status |

### 4.3 Error Handling

```typescript
// Standardized error handling in api.ts
export interface ApiErrorResponse {
  error: {
    code: string;
    message: string;
  };
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public code?: string
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

// Usage in components
try {
  const result = await api.search.distributed({ query, scope: 'all' });
  setResults(result);
} catch (error) {
  if (error instanceof ApiError) {
    if (error.status === 400) {
      setError('Invalid search query');
    } else if (error.status === 503) {
      setError('Search service unavailable');
    } else {
      setError('Search failed. Please try again.');
    }
  }
}
```

---

## 5. Infrastructure & Deployment

### 5.1 Build Configuration

No changes required to Vite configuration. Existing setup supports:
- TypeScript compilation
- CSS modules
- Development hot reload
- Production bundling

### 5.2 Environment Variables

No new environment variables required. All configuration uses existing:
- `VITE_API_URL` (optional, defaults to `http://localhost:8080`)

### 5.3 Bundle Size Considerations

| Addition | Estimated Size | Notes |
|----------|---------------|-------|
| New components | ~15 KB | CSS + TSX |
| Custom hooks | ~3 KB | Logic only |
| Types | 0 KB | Compile-time only |
| **Total** | ~18 KB | Minimal impact |

---

## 6. Security Architecture

### 6.1 Authentication

All new API endpoints require authentication via existing JWT token mechanism:

```typescript
// Existing pattern in api.ts - no changes needed
const token = localStorage.getItem('token');
if (token) {
  headers['Authorization'] = `Bearer ${token}`;
}
```

### 6.2 Input Validation

| Input | Validation | Location |
|-------|------------|----------|
| Search query | Max 1000 chars, trim whitespace | SearchBar component |
| Scope selector | Enum validation ('local', 'network', 'all') | ScopeSelector component |
| Space key | Alphanumeric + hyphen | Backend (trusted) |

### 6.3 XSS Prevention

- React's default escaping handles user-generated content
- Search results (titles, snippets) rendered as text, not HTML
- No raw HTML rendering of user content

---

## 7. Integration Architecture

### 7.1 Backend API Integration

**Request/Response Contract:**

```
Frontend                           Backend
   │                                  │
   │  POST /api/v1/search/distributed │
   │  {query, scope, limit}           │
   │ ────────────────────────────────►│
   │                                  │
   │  {success, results[], counts...} │
   │◄────────────────────────────────│
   │                                  │
```

**Error Response Format:**

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Query cannot be empty"
  }
}
```

### 7.2 Polling Strategy

| Component | Interval | Condition |
|-----------|----------|-----------|
| NetworkIndicator | 30 seconds | Always when authenticated |
| SyncStatus | 5 seconds | When `indexing_in_progress === true` |
| SyncStatus | 60 seconds | When idle (no active indexing) |

```typescript
// Example polling implementation with proper cleanup
function useSyncStatus(spaceKey: string) {
  const [status, setStatus] = useState<SpaceStatus | null>(null);
  const intervalRef = useRef<number | null>(null);

  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const data = await api.spaces.getStatus(spaceKey);
        setStatus(data);
      } catch (error) {
        console.error('Failed to fetch status:', error);
      }
    };

    // Clear existing interval before setting new one
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
    }

    fetchStatus();

    // Faster polling during indexing
    const pollInterval = status?.indexing_in_progress ? 5000 : 60000;
    intervalRef.current = window.setInterval(fetchStatus, pollInterval);

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [spaceKey, status?.indexing_in_progress]);

  return status;
}
```

---

## 8. Performance & Scalability

### 8.1 Performance Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Search results render | < 500ms | After API response received |
| Initial page load | < 2s | Time to interactive |
| Network indicator update | < 100ms | After API response |
| Publish modal open | < 100ms | Click to visible |

### 8.2 Optimization Strategies

**1. Debounced Search Input**
```typescript
const debouncedQuery = useDebouncedValue(query, 300);

useEffect(() => {
  if (debouncedQuery) {
    performSearch(debouncedQuery);
  }
}, [debouncedQuery]);
```

**2. Memoized Result Cards**
```typescript
const ResultCard = memo(function ResultCard({ result }: { result: SearchResult }) {
  return (
    <div className="result-card">
      {/* ... */}
    </div>
  );
});
```

**3. Virtualized List (if > 100 results)**
```typescript
// Only implement if performance issues observed
// Use react-window or similar if needed
```

### 8.3 Caching Strategy

| Data | Cache Location | TTL | Invalidation |
|------|----------------|-----|--------------|
| Network status | NetworkContext | 30s | Auto-refresh |
| Space status | Component state | 5-60s | Manual refresh |
| Search results | None | N/A | Always fresh |

---

## 9. Reliability & Operations

### 9.1 Error States

| Component | Error State | Recovery |
|-----------|-------------|----------|
| SearchPage | "Search failed" message | Retry button |
| NetworkIndicator | "Unknown" status | Auto-retry on next poll |
| SyncStatus | "Error fetching status" | Manual refresh |
| PublishButton | "Publish failed" message | Retry in modal |

### 9.2 Loading States

```typescript
type LoadingState = 'idle' | 'loading' | 'success' | 'error';

function SearchPage() {
  const [state, setState] = useState<LoadingState>('idle');

  // Render based on state
  if (state === 'loading') return <SearchSkeleton />;
  if (state === 'error') return <SearchError onRetry={retry} />;
  // ...
}
```

### 9.3 Offline Handling

```typescript
// Detect offline state
const isOnline = navigator.onLine;

// Disable network-dependent features when offline
<ScopeSelector
  disabled={!isOnline}
  disabledOptions={isOnline ? [] : ['network', 'all']}
/>
```

---

## 10. Development Standards

### 10.1 File Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Components | PascalCase | `SearchBar.tsx` |
| Hooks | camelCase with `use` prefix | `useDistributedSearch.ts` |
| CSS | Match component name | `SearchBar.css` |
| Tests | Match source with `.test` | `SearchBar.test.tsx` |
| Types | PascalCase | `SearchResult` |

### 10.2 Component Structure

```typescript
// Standard component structure
import { useState, useEffect, memo } from 'react';
import './ComponentName.css';

interface ComponentNameProps {
  // Props with JSDoc comments
  /** The search query to execute */
  query: string;
  /** Callback when search completes */
  onComplete?: (results: SearchResult[]) => void;
}

/**
 * Brief description of component purpose.
 */
export function ComponentName({ query, onComplete }: ComponentNameProps) {
  // State declarations
  const [results, setResults] = useState<SearchResult[]>([]);

  // Effects
  useEffect(() => {
    // ...
  }, [query]);

  // Event handlers
  const handleClick = () => {
    // ...
  };

  // Render
  return (
    <div className="component-name">
      {/* ... */}
    </div>
  );
}
```

### 10.3 CSS Conventions

```css
/* Component-scoped CSS using BEM-like naming */
.search-bar {
  /* Container styles */
}

.search-bar__input {
  /* Element styles */
}

.search-bar__input--focused {
  /* Modifier styles */
}

/* CSS custom properties for theming */
.search-bar {
  --search-bar-bg: var(--color-surface);
  --search-bar-border: var(--color-border);
  background: var(--search-bar-bg);
  border: 1px solid var(--search-bar-border);
}
```

### 10.4 Testing Standards

```typescript
// Test file structure
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { vi } from 'vitest';
import { SearchBar } from './SearchBar';

// Mock API
vi.mock('../services/api', () => ({
  api: {
    search: {
      distributed: vi.fn(),
    },
  },
}));

describe('SearchBar', () => {
  // Setup/teardown
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // Test cases
  it('renders search input', () => {
    render(<SearchBar onSearch={vi.fn()} />);
    expect(screen.getByPlaceholderText(/search/i)).toBeInTheDocument();
  });

  it('calls onSearch with debounced query', async () => {
    const onSearch = vi.fn();
    render(<SearchBar onSearch={onSearch} />);

    fireEvent.change(screen.getByRole('textbox'), {
      target: { value: 'test query' },
    });

    await waitFor(() => {
      expect(onSearch).toHaveBeenCalledWith('test query');
    }, { timeout: 500 });
  });

  // Edge cases
  it('does not search with empty query', () => {
    // ...
  });
});
```

### 10.5 Accessibility Requirements

| Requirement | Implementation |
|-------------|----------------|
| Keyboard navigation | All interactive elements focusable |
| Screen reader | ARIA labels on buttons, status regions |
| Focus management | Focus trap in modals |
| Color contrast | 4.5:1 minimum ratio |
| Error announcements | `aria-live` regions for errors |

```typescript
// Example accessible component
<button
  aria-label="Publish space to network"
  aria-pressed={isPublished}
  onClick={handlePublish}
>
  {isPublished ? 'Published' : 'Publish'}
</button>

<div
  role="status"
  aria-live="polite"
  aria-atomic="true"
>
  {searchStatus}
</div>
```

---

## 11. Implementation Roadmap

### 11.1 Work Items

| ID | Title | Priority | Dependencies | Estimate |
|----|-------|----------|--------------|----------|
| UI-1 | Create SearchPage component | P0 | None | 4h |
| UI-2 | Create SearchBar component | P0 | None | 2h |
| UI-3 | Create ScopeSelector component | P0 | None | 1h |
| UI-4 | Create ResultCard component | P0 | None | 2h |
| UI-5 | Create SearchResults component | P0 | UI-4 | 2h |
| UI-6 | Add search API methods | P0 | None | 1h |
| UI-7 | Add useDistributedSearch hook | P0 | UI-6 | 2h |
| UI-8 | Integrate search page routing | P0 | UI-1, UI-7 | 1h |
| UI-9 | Add search link to Layout | P0 | UI-8 | 0.5h |
| UI-10 | Create PublishButton component | P0 | None | 2h |
| UI-11 | Create PublishConfirmModal | P0 | None | 3h |
| UI-12 | Add publish API methods | P0 | None | 1h |
| UI-13 | Integrate publish into SpacePage | P0 | UI-10, UI-11, UI-12 | 2h |
| UI-14 | Create SyncStatus component | P0 | None | 3h |
| UI-15 | Create SyncProgress component | P1 | None | 2h |
| UI-16 | Add useSpaceStatus hook | P0 | None | 1h |
| UI-17 | Integrate sync status into SpacePage | P0 | UI-14, UI-16 | 1h |
| UI-18 | Create NetworkIndicator component | P0 | None | 2h |
| UI-19 | Create NetworkContext | P0 | None | 2h |
| UI-20 | Add network API methods | P0 | None | 0.5h |
| UI-21 | Integrate NetworkIndicator in Layout | P0 | UI-18, UI-19 | 1h |
| UI-22 | Create NetworkDetails component | P2 | None | 2h |
| UI-23 | Add network section to Settings | P2 | UI-22 | 1h |
| UI-24 | Add keyboard shortcut for search | P1 | UI-8 | 1h |
| UI-25 | Write unit tests for search components | P0 | UI-1 through UI-5 | 4h |
| UI-26 | Write unit tests for publish components | P0 | UI-10, UI-11 | 3h |
| UI-27 | Write unit tests for sync components | P0 | UI-14, UI-15 | 2h |
| UI-28 | Write unit tests for network components | P0 | UI-18, UI-19 | 2h |
| UI-29 | Write unit tests for custom hooks | P0 | UI-7, UI-16 | 2h |
| UI-30 | Create NetworkResultPreview component | P0 | UI-4 | 2h |

### 11.2 Implementation Phases

```
Phase 1: Search Foundation (UI-1 through UI-9)
├── SearchPage shell
├── SearchBar with input
├── ScopeSelector
├── API integration
├── ResultCard
├── SearchResults list
└── Routing and navigation

Phase 2: Publishing (UI-10 through UI-13)
├── PublishButton
├── PublishConfirmModal
├── API methods
└── SpacePage integration

Phase 3: Sync Status (UI-14 through UI-17)
├── SyncStatus component
├── SyncProgress bar
├── useSpaceStatus hook
└── SpacePage integration

Phase 4: Network Status (UI-18 through UI-23)
├── NetworkIndicator
├── NetworkContext
├── API methods
├── Layout integration
├── NetworkDetails
└── Settings integration

Phase 5: Polish & Testing (UI-24 through UI-28)
├── Keyboard shortcuts
├── Unit tests
└── Accessibility audit
```

### 11.3 Definition of Done (Per Work Item)

- [ ] Component implemented per spec
- [ ] CSS styles applied
- [ ] TypeScript types defined
- [ ] Unit tests written and passing
- [ ] Accessibility requirements met
- [ ] Code reviewed

---

## 12. Appendices

### 12.1 Component Specifications

#### A. SearchBar Component

```typescript
interface SearchBarProps {
  /** Initial query value */
  initialQuery?: string;
  /** Callback when query changes (debounced) */
  onQueryChange: (query: string) => void;
  /** Whether search is in progress */
  isSearching?: boolean;
  /** Placeholder text */
  placeholder?: string;
}
```

**Behavior:**
- Debounce input by 300ms
- Clear button appears when query is not empty
- Loading spinner replaces search icon during search
- Submit on Enter key

#### B. ScopeSelector Component

```typescript
interface ScopeSelectorProps {
  /** Current selected scope */
  value: SearchScope;
  /** Callback when scope changes */
  onChange: (scope: SearchScope) => void;
  /** Disabled state */
  disabled?: boolean;
}
```

**Options:**
- "All" (default) - searches local + network
- "Local Only" - searches local spaces only
- "Network Only" - searches network peers only

#### C. ResultCard Component

```typescript
interface ResultCardProps {
  /** Search result data */
  result: SearchResult;
  /** Click handler */
  onClick?: () => void;
}
```

**Display:**
- Source badge (LOCAL/NETWORK)
- Title (or "Untitled" if missing)
- Snippet (first 150 chars)
- Score (as percentage)
- Source identifier (space name or peer ID)

#### D. PublishButton Component

```typescript
interface PublishButtonProps {
  /** Space key */
  spaceKey: string;
  /** Whether space is currently published */
  isPublished: boolean;
  /** Published timestamp */
  publishedAt?: string;
  /** Callback after publish/unpublish */
  onStatusChange: (isPublished: boolean) => void;
}
```

**States:**
- Unpublished: "Publish" button
- Published: "Published" dropdown with "Unpublish" option
- Loading: Spinner during operation

#### E. SyncStatus Component

```typescript
interface SyncStatusProps {
  /** Space key to fetch status for */
  spaceKey: string;
  /** Whether to show re-index button */
  showReindex?: boolean;
}
```

**Display:**
- Status badge (Synced/Syncing/Error)
- Files indexed count
- Chunks stored count
- Last indexed timestamp
- Error details (expandable)
- Re-index button

#### F. NetworkIndicator Component

```typescript
interface NetworkIndicatorProps {
  /** Whether to show peer count */
  showPeerCount?: boolean;
  /** Click handler to open details */
  onClick?: () => void;
}
```

**Display:**
- Connected: Green dot + peer count
- Disconnected: Red dot + "Offline"
- Unknown: Gray dot + "..."

#### G. NetworkResultPreview Component

```typescript
interface NetworkResultPreviewProps {
  /** The search result to preview */
  result: SearchResult;
  /** Whether the preview is visible */
  isOpen: boolean;
  /** Callback to close the preview */
  onClose: () => void;
}
```

**Display:**
- Slide-in panel or modal
- Full title (not truncated)
- Complete snippet text
- Source peer ID with copy button
- Published date (if available)
- "Copy CID" button for advanced users
- Close button (X) and Escape key support

### 12.2 CSS Variables Reference

```css
/* Colors (existing theme) */
--color-primary: #3b82f6;
--color-success: #22c55e;
--color-warning: #f59e0b;
--color-error: #ef4444;
--color-surface: #ffffff;
--color-surface-elevated: #f8fafc;
--color-border: #e2e8f0;
--color-text: #0f172a;
--color-text-secondary: #64748b;

/* New component-specific variables */
--badge-local-bg: #dbeafe;
--badge-local-text: #1e40af;
--badge-network-bg: #dcfce7;
--badge-network-text: #166534;
```

### 12.3 Keyboard Shortcuts

| Shortcut | Action | Context |
|----------|--------|---------|
| `Cmd/Ctrl + K` | Open search page | Global |
| `Escape` | Close modal | When modal open |
| `Enter` | Submit search | In search input |

### 12.4 Test Utilities

```typescript
// src/test/utils.tsx
import { render, RenderOptions } from '@testing-library/react';
import { BrowserRouter } from 'react-router-dom';
import { AuthProvider } from '../contexts/AuthContext';
import { NetworkProvider } from '../contexts/NetworkContext';

function AllProviders({ children }: { children: React.ReactNode }) {
  return (
    <BrowserRouter>
      <AuthProvider>
        <NetworkProvider>
          {children}
        </NetworkProvider>
      </AuthProvider>
    </BrowserRouter>
  );
}

export function renderWithProviders(
  ui: React.ReactElement,
  options?: Omit<RenderOptions, 'wrapper'>
) {
  return render(ui, { wrapper: AllProviders, ...options });
}
```

### 12.5 API Mock Fixtures

```typescript
// src/test/fixtures/search.ts
export const mockSearchResponse: DistributedSearchResponse = {
  success: true,
  query: 'test query',
  scope: 'all',
  results: [
    {
      cid: 'bafybeigdyrzt...',
      score: 0.92,
      source: 'local',
      title: 'Test Document',
      snippet: 'This is a test document containing...',
      source_id: 'my-space',
    },
    {
      cid: 'bafybeihxyz...',
      score: 0.85,
      source: 'network',
      title: 'Network Result',
      snippet: 'Content from network peer...',
      source_id: '12D3KooW...',
    },
  ],
  local_count: 1,
  network_count: 1,
  peers_queried: 3,
  peers_responded: 2,
  total_found: 2,
  elapsed_ms: 245,
};

export const mockNetworkStatus: NetworkStatus = {
  connected: true,
  peer_count: 3,
  peers: [
    { id: '12D3KooWabc...', connected_at: '2026-01-22T10:00:00Z' },
    { id: '12D3KooWdef...', connected_at: '2026-01-22T09:45:00Z' },
    { id: '12D3KooWghi...', connected_at: '2026-01-22T09:00:00Z' },
  ],
};
```
