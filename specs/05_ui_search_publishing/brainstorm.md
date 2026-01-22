# Brainstorm: UI for Search, Publishing & Network Features

**Date:** 2026-01-22
**Status:** Draft
**Cycle Type:** Feature Development - Frontend

---

## Executive Summary

Build comprehensive user interface components for Flow Network's recently implemented backend features:
1. **Distributed Search** - Search across local and network sources
2. **Content Publishing** - Publish local content to the network
3. **Sync Status** - Monitor data synchronization progress
4. **Network Status** - View peer connections and network health

These features complete the user-facing experience for Phase 2 & 3 backend work.

---

## Problem Statement

The Flow Network backend now supports:
- Distributed search across local spaces and network peers (Phase 3)
- Content publishing and data sync (Phase 2)
- Network peer discovery and connections

However, **users have no way to access these features** through the UI. The current interface only supports:
- Authentication (WebAuthn)
- Viewing local spaces
- Basic space-level search (single space only)
- Viewing entities within a space

Users need intuitive interfaces to:
- Search across ALL their content (local + network) in one place
- Publish their content to make it discoverable by others
- Monitor sync status and troubleshoot issues
- Understand their network connectivity

---

## Target Users

### Primary: Knowledge Workers
- Researchers, writers, developers who manage large document collections
- Need to find information quickly across multiple sources
- Want to share knowledge with collaborators

### Secondary: Team Collaborators
- People working in distributed teams
- Need to discover content published by peers
- Want to contribute to shared knowledge bases

### Tertiary: Power Users
- System administrators, developers
- Need visibility into sync status and network health
- Want to troubleshoot connectivity issues

---

## Feature Breakdown

### Feature 1: Distributed Search UI

#### User Stories
- As a user, I want to search across all my local spaces at once
- As a user, I want to search content from network peers
- As a user, I want to filter search by scope (local/network/all)
- As a user, I want to see relevance scores for results
- As a user, I want to identify which results are local vs network

#### Key Screens
1. **Global Search Page** (`/search`)
   - Search input with scope selector
   - Results list with source indicators
   - Performance metrics (search time, peer count)

2. **Search Results Component**
   - Result cards showing title, snippet, score
   - Visual distinction between local/network results
   - Click to navigate to content

#### Backend APIs Available
```
POST /api/v1/search/distributed
  Request: { query, scope: "local"|"network"|"all", limit }
  Response: { results[], local_count, network_count, elapsed_ms }

GET /api/v1/search/distributed/health
  Response: { status, cache: { entries, hits, misses } }
```

---

### Feature 2: Content Publishing UI

#### User Stories
- As a user, I want to publish a space to make it searchable by peers
- As a user, I want to see which spaces are published
- As a user, I want to unpublish content when needed
- As a user, I want to understand what publishing means (privacy implications)

#### Key Screens
1. **Space Page Enhancement**
   - Publish/Unpublish toggle or button
   - Publication status indicator
   - "Last published" timestamp

2. **Publishing Confirmation Modal**
   - Explain what gets shared
   - Privacy implications
   - Confirm/Cancel actions

#### Backend APIs Available
```
POST /api/v1/spaces/{key}/publish
  Response: { success, cid, published_at }

DELETE /api/v1/spaces/{key}/publish
  Response: { success }

GET /api/v1/spaces/{key}
  Response: { ...space, is_published, published_at }
```

---

### Feature 3: Sync Status UI

#### User Stories
- As a user, I want to see if my content is being indexed
- As a user, I want to see indexing progress (files processed)
- As a user, I want to know if there are indexing errors
- As a user, I want to manually trigger re-indexing

#### Key Screens
1. **Space Page Enhancement**
   - Sync status badge (syncing/synced/error)
   - Progress indicator during indexing
   - Error message display
   - "Re-index" button

2. **Status Details Panel**
   - Files indexed count
   - Chunks stored count
   - Last indexed timestamp
   - Error details if any

#### Backend APIs Available
```
GET /api/v1/spaces/{key}/status
  Response: {
    indexing_in_progress,
    last_indexed,
    files_indexed,
    chunks_stored,
    files_failed,
    last_error
  }

POST /api/v1/spaces/{key}/reindex
  Response: { status: "started" }
```

---

### Feature 4: Network Status UI

#### User Stories
- As a user, I want to see if I'm connected to the network
- As a user, I want to see how many peers I'm connected to
- As a user, I want to see network health at a glance

#### Key Screens
1. **Layout/Header Enhancement**
   - Network status indicator (connected/disconnected)
   - Peer count badge
   - Click to expand details

2. **Network Details Panel** (Settings page or modal)
   - Connection status
   - Peer list with addresses
   - Network statistics

#### Backend APIs Available
```
GET /api/v1/network/status
  Response: { connected, peer_count, peers[] }

GET /api/v1/health
  Response: { status, timestamp }
```

---

## Design Principles

### 1. Progressive Disclosure
- Show simple status at a glance
- Allow drilling into details on demand
- Don't overwhelm with technical information

### 2. Clear Feedback
- Every action should have visible feedback
- Loading states for async operations
- Success/error messages

### 3. Consistent Patterns
- Follow existing UI patterns in the codebase
- Reuse existing components where possible
- Maintain visual consistency

### 4. Accessibility
- Keyboard navigation support
- Screen reader friendly
- Sufficient color contrast

---

## Technical Context

### Existing Frontend Stack
- React 18 + TypeScript
- Vite for build
- React Router for navigation
- Vitest for testing
- CSS modules (no framework)

### Existing Patterns
- `api.ts` service for all API calls
- Pages in `pages/` directory
- Reusable components in `components/`
- Auth context for authentication state

### Constraints
- Must work in both web and Electron builds
- No additional UI framework (keep vanilla CSS)
- Must maintain existing test coverage standards

---

## Success Criteria

### Functional
- [ ] Users can search across local and network sources
- [ ] Users can publish/unpublish spaces
- [ ] Users can see sync status for each space
- [ ] Users can see network connectivity status

### Non-Functional
- [ ] Search results render in < 500ms after API response
- [ ] All new components have unit tests
- [ ] Accessible (WCAG AA compliance)
- [ ] Works in Electron desktop app

### User Experience
- [ ] Clear visual distinction between local/network results
- [ ] Intuitive publish flow with privacy explanation
- [ ] At-a-glance status indicators
- [ ] Helpful error messages

---

## Open Questions

1. **Search Page Location:** Should search be a dedicated page (`/search`) or integrated into the home page?
   - **Recommendation:** Dedicated page with quick access from header

2. **Publishing Granularity:** Publish entire space or individual files?
   - **Recommendation:** Start with space-level (matches backend)

3. **Network Status Placement:** Header indicator vs settings page?
   - **Recommendation:** Both - indicator in header, details in settings

4. **Error Handling:** How verbose should error messages be?
   - **Recommendation:** User-friendly with "show details" option

---

## Out of Scope (Future Work)

- Real-time search suggestions/autocomplete
- Advanced search filters (date, type, etc.)
- Bulk publishing operations
- Network peer management (block/allow)
- Offline mode indicators
- Search history

---

## References

- Backend APIs: `back-end/node/src/api/servers/rest/`
- Existing UI: `user-interface/flow-web/src/`
- Phase 3 Spec: `specs/04_phase3_distributed_search/`
- Phase 2 Spec: `specs/03_phase2_completion/`
