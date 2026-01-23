# Product Specification: UI for Search, Publishing & Network Features

**Version:** 1.0
**Date:** 2026-01-22
**Status:** Draft
**Author:** Claude (AI Assistant)

---

## 1. Executive Summary

### 1.1 Product Vision

Deliver intuitive user interface components that enable Flow Network users to:
- **Search** across all their content (local and network) from a single interface
- **Publish** their knowledge to make it discoverable by network peers
- **Monitor** synchronization status and troubleshoot issues
- **Understand** their network connectivity at a glance

### 1.2 Problem Statement

Flow Network's backend now supports distributed search, content publishing, and peer-to-peer networking. However, users cannot access these powerful features because no UI exists. Users are limited to:
- Viewing individual spaces
- Searching within a single space
- No visibility into network status or sync progress

### 1.3 Target Outcome

A polished, accessible UI that transforms Flow from a local-only tool into a fully networked knowledge platform, enabling users to leverage the distributed capabilities built in Phase 2 and Phase 3.

---

## 2. User Personas

### 2.1 Primary: Alex the Knowledge Worker

**Demographics:** 32, Senior Researcher at a tech company
**Technical Level:** Intermediate
**Goals:**
- Find information quickly across multiple document collections
- Share research findings with team members
- Stay organized without manual effort

**Pain Points:**
- Currently must search each space individually
- No way to share content with colleagues
- Doesn't know if content is properly indexed

**Quote:** "I just want to type what I'm looking for and get answers, regardless of where the information lives."

### 2.2 Secondary: Morgan the Team Lead

**Demographics:** 45, Engineering Manager
**Technical Level:** Advanced
**Goals:**
- Enable team knowledge sharing
- Monitor team's published content
- Ensure system is working correctly

**Pain Points:**
- No visibility into what's shared vs private
- Can't troubleshoot when things go wrong
- Team members ask "is the system working?"

**Quote:** "I need to know at a glance if everything is healthy and what my team has published."

### 2.3 Tertiary: Sam the Power User

**Demographics:** 28, DevOps Engineer
**Technical Level:** Expert
**Goals:**
- Understand system internals
- Debug connectivity issues
- Optimize performance

**Pain Points:**
- No network diagnostics
- Can't see peer connections
- Limited error information

**Quote:** "Show me the details - I can handle the complexity."

---

## 3. Feature Specifications

### 3.1 Feature: Distributed Search UI

#### 3.1.1 Overview

A dedicated search page that queries across all local spaces and network peers, presenting unified results with clear source attribution.

#### 3.1.2 User Stories

| ID | Story | Priority |
|----|-------|----------|
| US-1.1 | As a user, I want a global search page so I can find content across all my spaces | P0 |
| US-1.2 | As a user, I want to filter by scope (local/network/all) so I can control where I search | P0 |
| US-1.3 | As a user, I want to see which results are local vs network so I know the source | P0 |
| US-1.4 | As a user, I want to see relevance scores so I can prioritize results | P1 |
| US-1.5 | As a user, I want to see search performance metrics so I understand latency | P2 |
| US-1.6 | As a user, I want to access search quickly from anywhere in the app | P0 |
| US-1.7 | As a user, I want to interact with search results so I can view or access content | P0 |

#### 3.1.3 Acceptance Criteria

**US-1.1: Global Search Page**
- [ ] Search page accessible at `/search` route
- [ ] Search input field with placeholder text "Search your knowledge..."
- [ ] Results display below search input (max 20 results initially, "Load more" for additional)
- [ ] Empty state when no results found with suggestions: "Try broader terms" or "Adjust scope filter"
- [ ] Loading state while search in progress

**US-1.2: Scope Filter**
- [ ] Dropdown/toggle to select: All, Local Only, Network Only
- [ ] Default scope is "All"
- [ ] Scope selection persists during session
- [ ] Scope reflected in search request

**US-1.3: Source Attribution**
- [ ] Each result card shows source badge (Local/Network)
- [ ] Local results have distinct visual style (e.g., blue badge)
- [ ] Network results have distinct visual style (e.g., green badge)
- [ ] Summary shows count: "X local, Y network results"

**US-1.4: Relevance Scores**
- [ ] Score displayed on each result (0-100% or similar)
- [ ] Results sorted by score descending
- [ ] Score tooltip explains what it means

**US-1.5: Performance Metrics**
- [ ] Display "Found X results in Yms"
- [ ] Show peers queried count for network searches
- [ ] Metrics appear after results load

**US-1.6: Quick Access**
- [ ] Search icon in header/navigation
- [ ] Keyboard shortcut (Cmd/Ctrl + K) opens search (with fallback to Cmd/Ctrl + / if conflicts detected)
- [ ] Search accessible from any page

**US-1.7: Result Interaction**
- [ ] **Local results:** Click navigates to the source space with content highlighted/scrolled to
- [ ] **Network results:** Click opens a preview panel showing full snippet and metadata
- [ ] Network preview includes: title, full snippet, source peer ID, published date
- [ ] Network preview has "Copy CID" button for advanced users
- [ ] Results support keyboard navigation (arrow keys, Enter to select)

#### 3.1.4 UI Mockup Description

```
┌─────────────────────────────────────────────────────────────┐
│  [Flow Logo]   Home   Search   Settings      [Network: ●]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│           🔍 Search your knowledge...          [All ▼]     │
│                                                             │
│  ─────────────────────────────────────────────────────────  │
│                                                             │
│  Found 12 results (8 local, 4 network) in 245ms            │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ [LOCAL]  Project Architecture Notes           92%   │   │
│  │ This document describes the overall system...       │   │
│  │ Space: tech-docs • Last updated: 2 days ago        │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ [NETWORK] Distributed Systems Guide           87%   │   │
│  │ A comprehensive guide to building distributed...    │   │
│  │ From: peer-abc123 • Published: 1 week ago          │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ [LOCAL]  API Design Patterns                  85%   │   │
│  │ Best practices for RESTful API design...           │   │
│  │ Space: engineering • Last updated: 5 days ago      │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.1.5 Edge Cases

- **No results:** Show friendly empty state with suggestions ("Try broader terms", "Check scope")
- **Search error:** Show error message with retry option
- **Network timeout:** Show partial results with warning
- **Very long query:** Truncate display, full query in tooltip
- **Special characters:** Escape properly, don't break search
- **Rate limiting:** Max 10 searches per minute; show "Please wait" message if exceeded
- **Many results:** Initial display capped at 20; "Load more" button fetches next batch

---

### 3.2 Feature: Content Publishing UI

#### 3.2.1 Overview

Enable users to publish their spaces to the network, making content discoverable by peers. Includes clear privacy explanations and publication status tracking.

#### 3.2.2 User Stories

| ID | Story | Priority |
|----|-------|----------|
| US-2.1 | As a user, I want to publish a space so peers can discover my content | P0 |
| US-2.2 | As a user, I want to see which spaces are published so I know what's shared | P0 |
| US-2.3 | As a user, I want to unpublish a space so I can make it private again | P0 |
| US-2.4 | As a user, I want to understand what publishing means before I do it | P0 |
| US-2.5 | As a user, I want to see when a space was last published | P1 |

#### 3.2.3 Acceptance Criteria

**US-2.1: Publish Space**
- [ ] "Publish" button visible on Space page for unpublished spaces
- [ ] Clicking opens confirmation modal
- [ ] Modal explains what gets shared
- [ ] Success feedback after publishing
- [ ] Button changes to "Published" state

**US-2.2: Publication Status**
- [ ] Published spaces show "Published" badge
- [ ] Unpublished spaces show no badge or "Private" indicator
- [ ] Status visible in space list (Home page) and Space page

**US-2.3: Unpublish Space**
- [ ] "Unpublish" option available for published spaces
- [ ] Confirmation dialog before unpublishing
- [ ] Success feedback after unpublishing
- [ ] Status updates immediately

**US-2.4: Privacy Explanation**
- [ ] Modal includes clear explanation of what publishing does
- [ ] Bullet points: what's shared, who can see it, how to undo
- [ ] Link to documentation/help for more details
- [ ] Checkbox: "I understand" before enabling Publish button

**US-2.5: Publication Timestamp**
- [ ] "Published on [date]" shown for published spaces
- [ ] Relative time format (e.g., "2 days ago")
- [ ] Hover/click for exact timestamp

#### 3.2.4 UI Mockup Description

**Space Page - Unpublished State:**
```
┌─────────────────────────────────────────────────────────────┐
│  ← Back    My Research Notes                    [Publish]   │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  📁 /Users/alex/Documents/Research                         │
│                                                             │
│  Status: Private • 142 files indexed • Last sync: 5m ago   │
│                                                             │
```

**Space Page - Published State:**
```
┌─────────────────────────────────────────────────────────────┐
│  ← Back    My Research Notes          [✓ Published ▼]      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  📁 /Users/alex/Documents/Research                         │
│                                                             │
│  Status: Published • 142 files • Published 2 days ago      │
│                                                             │
│  ┌─────────────────────────┐                               │
│  │ ✓ Published             │                               │
│  │ ─────────────────────── │                               │
│  │   Unpublish             │                               │
│  └─────────────────────────┘                               │
```

**Publish Confirmation Modal:**
```
┌─────────────────────────────────────────────────────────────┐
│                    Publish This Space?                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Publishing makes your content discoverable by network      │
│  peers. Here's what happens:                               │
│                                                             │
│  ✓ Your indexed content becomes searchable                 │
│  ✓ Peers can find results from your space                  │
│  ✓ Only search results are shared, not raw files           │
│  ✓ You can unpublish anytime                               │
│                                                             │
│  ⚠️  Anyone on the network can discover published content  │
│                                                             │
│  ☐ I understand what publishing means                      │
│                                                             │
│              [Cancel]            [Publish]                  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.2.5 Edge Cases

- **Publish fails:** Show error with reason and retry option
- **Space not indexed:** Prompt to index before publishing
- **Large space:** Show progress during publish operation
- **Network disconnected:** Disable publish, show explanation
- **Unpublish propagation:** Note that content may remain discoverable for a short period (up to 5 minutes) after unpublishing due to network caching
- **Concurrent operations:** Disable publish button if reindex is in progress

---

### 3.3 Feature: Sync Status UI

#### 3.3.1 Overview

Display real-time indexing status for each space, including progress indicators, error messages, and manual re-index capability.

#### 3.3.2 User Stories

| ID | Story | Priority |
|----|-------|----------|
| US-3.1 | As a user, I want to see if a space is currently being indexed | P0 |
| US-3.2 | As a user, I want to see indexing progress (files processed) | P1 |
| US-3.3 | As a user, I want to know if there are indexing errors | P0 |
| US-3.4 | As a user, I want to manually trigger re-indexing | P1 |
| US-3.5 | As a user, I want to see when a space was last successfully indexed | P1 |

#### 3.3.3 Acceptance Criteria

**US-3.1: Indexing Status Indicator**
- [ ] Status badge: Synced (green), Syncing (blue/animated), Error (red)
- [ ] Visible on Space page header
- [ ] Visible in space list on Home page
- [ ] Updates in real-time (poll every 5 seconds during sync)

**US-3.2: Progress Display**
- [ ] "Indexing: X of Y files" shown during sync
- [ ] Progress bar or percentage indicator
- [ ] Estimated time remaining (optional, P2)

**US-3.3: Error Display**
- [ ] Error badge visible when last sync had errors
- [ ] "X files failed" count shown
- [ ] Click to see error details
- [ ] Error details include file names and reasons

**US-3.4: Manual Re-index**
- [ ] "Re-index" button on Space page
- [ ] Confirmation if space is large (>1000 files)
- [ ] Button disabled during active indexing
- [ ] Success feedback when started

**US-3.5: Last Indexed Timestamp**
- [ ] "Last indexed: [relative time]" shown
- [ ] Hover for exact timestamp
- [ ] "Never" if not yet indexed

#### 3.3.4 UI Mockup Description

**Space Page - Sync Status Section:**
```
┌─────────────────────────────────────────────────────────────┐
│  Sync Status                                    [Re-index]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ● Synced                                                   │
│                                                             │
│  Files indexed: 142                                         │
│  Chunks stored: 1,847                                       │
│  Last indexed: 5 minutes ago                               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Syncing State:**
```
┌─────────────────────────────────────────────────────────────┐
│  Sync Status                                                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ◐ Syncing...                                              │
│                                                             │
│  Progress: 87 of 142 files (61%)                           │
│  ████████████░░░░░░░░                                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Error State:**
```
┌─────────────────────────────────────────────────────────────┐
│  Sync Status                                    [Re-index]  │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ● Synced with errors                                      │
│                                                             │
│  Files indexed: 139                                         │
│  Files failed: 3  [View details]                           │
│  Last indexed: 2 hours ago                                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.3.5 Edge Cases

- **Very large space:** Show warning about time required
- **All files fail:** Show prominent error, suggest troubleshooting
- **Indexing stuck:** Timeout after reasonable period, show error
- **Space deleted during sync:** Handle gracefully, clear status

---

### 3.4 Feature: Network Status UI

#### 3.4.1 Overview

A lightweight indicator showing network connectivity and peer connections, with access to detailed network information.

#### 3.4.2 User Stories

| ID | Story | Priority |
|----|-------|----------|
| US-4.1 | As a user, I want to see if I'm connected to the network | P0 |
| US-4.2 | As a user, I want to see how many peers I'm connected to | P1 |
| US-4.3 | As a user, I want to see network details when needed | P2 |

#### 3.4.3 Acceptance Criteria

**US-4.1: Connection Indicator**
- [ ] Icon in header showing connected (green) or disconnected (red)
- [ ] Tooltip on hover with status text
- [ ] Updates automatically when status changes

**US-4.2: Peer Count**
- [ ] Number badge next to network icon
- [ ] "3 peers" or similar text on hover
- [ ] Updates periodically (every 30 seconds)

**US-4.3: Network Details**
- [ ] Click network icon to see details panel/modal
- [ ] Or: Network section in Settings page
- [ ] Shows: connection status, peer count, peer list (optional)

#### 3.4.4 UI Mockup Description

**Header Network Indicator:**
```
┌─────────────────────────────────────────────────────────────┐
│  [Flow Logo]   Home   Search   Settings      [🌐 3]  [👤]  │
└─────────────────────────────────────────────────────────────┘
                                                    ↑
                                          Network: Connected
                                          3 peers online
```

**Network Details (Settings Page Section):**
```
┌─────────────────────────────────────────────────────────────┐
│  Network                                                    │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Status: ● Connected                                        │
│  Peers: 3 online                                           │
│                                                             │
│  Your Peer ID: 12D3KooW...xyz                              │
│                                                             │
│  Connected Peers:                                          │
│  • 12D3KooW...abc (2 minutes)                              │
│  • 12D3KooW...def (15 minutes)                             │
│  • 12D3KooW...ghi (1 hour)                                 │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### 3.4.5 Edge Cases

- **No peers:** Show "No peers connected" with explanation
- **Network disabled:** Show "Offline mode" indicator
- **Connection lost:** Update indicator, optionally notify user
- **Many peers:** Truncate list with "and X more"

---

## 4. Information Architecture

### 4.1 Navigation Structure

```
/                     → Home (Space List)
/search               → Global Search (NEW)
/spaces/:key          → Space Detail (ENHANCED with publish/sync)
/settings             → Settings (ENHANCED with network status)
/auth                 → Authentication
```

### 4.2 Component Hierarchy

```
App
├── Layout
│   ├── Header
│   │   ├── Navigation
│   │   ├── SearchShortcut (NEW)
│   │   └── NetworkIndicator (NEW)
│   └── Main Content
│       ├── HomePage
│       │   └── SpaceCard (ENHANCED with status badges)
│       ├── SearchPage (NEW)
│       │   ├── SearchBar
│       │   ├── ScopeSelector
│       │   ├── SearchResults
│       │   └── ResultCard
│       ├── SpacePage (ENHANCED)
│       │   ├── SpaceHeader
│       │   ├── PublishButton (NEW)
│       │   ├── SyncStatus (NEW)
│       │   └── ...existing content
│       └── SettingsPage (ENHANCED)
│           └── NetworkStatus (NEW)
└── Modals
    ├── PublishConfirmModal (NEW)
    └── ErrorDetailsModal (NEW)
```

---

## 5. Non-Functional Requirements

### 5.1 Performance

| Metric | Target |
|--------|--------|
| Search results render | < 500ms after API response |
| Initial page load | < 2s |
| Status polling interval | 5s during sync (with exponential backoff on errors), 30s otherwise |
| Network indicator update | < 1s on status change |
| Search rate limit | Max 10 requests per minute per user |

### 5.2 Accessibility

- WCAG 2.1 AA compliance
- Keyboard navigation for all interactive elements
- Screen reader support with proper ARIA labels
- Color contrast ratio ≥ 4.5:1
- Focus indicators visible

### 5.3 Browser Support

- Chrome (latest 2 versions)
- Firefox (latest 2 versions)
- Safari (latest 2 versions)
- Edge (latest 2 versions)
- Electron (bundled Chromium)

### 5.4 Responsiveness

- Desktop-first design (primary use case)
- Minimum supported width: 1024px
- Graceful degradation on smaller screens

---

## 6. Success Metrics

### 6.1 Adoption Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Search page visits | 50% of active users | Analytics |
| Spaces published | 20% of spaces | Database query |
| Search queries/user/day | ≥ 3 | Analytics |

### 6.2 Quality Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Test coverage | ≥ 80% | Coverage report |
| Accessibility score | ≥ 90 (Lighthouse) | Audit |
| Error rate | < 1% of searches | Error tracking |

### 6.3 User Satisfaction

| Metric | Target | Measurement |
|--------|--------|-------------|
| Task completion rate | ≥ 95% | User testing |
| Time to first search | < 30s for new users | User testing |
| Publish flow completion | ≥ 90% | Analytics |

---

## 7. Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Backend API changes | High | Low | Lock API contract, version endpoints |
| Performance issues with many results | Medium | Medium | Pagination, virtualized lists |
| User confusion about publishing | High | Medium | Clear explanations, confirmation flow |
| Network status inaccurate | Low | Low | Polling, fallback states |

---

## 8. Release Strategy

### 8.1 Phased Rollout

**Phase 1: Core Search**
- Search page with basic functionality
- Scope selector
- Results display

**Phase 2: Publishing**
- Publish/unpublish functionality
- Status indicators
- Confirmation flow

**Phase 3: Status & Network**
- Sync status panel
- Network indicator
- Settings enhancements

### 8.2 Feature Flags (Optional)

- `ENABLE_DISTRIBUTED_SEARCH`: Toggle search page
- `ENABLE_PUBLISHING`: Toggle publish functionality
- `ENABLE_NETWORK_STATUS`: Toggle network indicator

---

## 9. Open Questions (Resolved)

| Question | Decision | Rationale |
|----------|----------|-----------|
| Search page location | Dedicated `/search` page | Cleaner UX, room for features |
| Publishing granularity | Space-level | Matches backend, simpler UX |
| Network status placement | Header indicator + settings details | Quick glance + deep dive |
| Error verbosity | User-friendly with expandable details | Balance simplicity and debugging |

---

## 10. Appendix

### 10.1 Backend API Reference

**Distributed Search:**
```
POST /api/v1/search/distributed
Request: { query: string, scope: "local"|"network"|"all", limit: number }
Response: {
  success: boolean,
  query: string,
  scope: string,
  results: Array<{
    cid: string,
    score: number,
    source: "local"|"network",
    title?: string,
    snippet?: string,
    source_id?: string
  }>,
  local_count: number,
  network_count: number,
  peers_queried: number,
  peers_responded: number,
  total_found: number,
  elapsed_ms: number
}
```

**Space Status:**
```
GET /api/v1/spaces/{key}/status
Response: {
  indexing_in_progress: boolean,
  last_indexed: string | null,
  files_indexed: number,
  chunks_stored: number,
  files_failed: number,
  last_error: string | null
}
```

**Publish/Unpublish:**
```
POST /api/v1/spaces/{key}/publish
DELETE /api/v1/spaces/{key}/publish
```

**Network Status:**
```
GET /api/v1/network/status
Response: {
  connected: boolean,
  peer_count: number,
  peers: Array<{ id: string, connected_at: string }>
}
```

### 10.2 Glossary

| Term | Definition |
|------|------------|
| Space | A local directory indexed by Flow |
| Publish | Make a space's content searchable by network peers |
| Sync | User-facing term for the indexing process; displayed in UI as "Synced", "Syncing" |
| Index | Technical process of analyzing files and creating search embeddings; used interchangeably with Sync in user context |
| Peer | Another Flow node on the network |
| CID | Content Identifier (content-addressed hash) |
| Scope | Search filter that determines where to search: "Local" (your spaces only), "Network" (peer content only), or "All" (both) |
