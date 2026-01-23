# Flow Network - Manual User Test Script

**Epic:** 05_ui_search_publishing
**Date:** 2026-01-22
**Tester:** ________________
**Environment:** Local Development

---

## Prerequisites

Before starting, ensure:

```bash
# One command to start everything (from project root)
npx nx start-all

# This starts:
# - Docker services (Qdrant on :6333, Redis on :6379)
# - Backend (Rust node on :8080)
# - Frontend (React on :3000)
```

**Alternative (manual start):**
```bash
# Terminal 1: Start infrastructure
cd back-end && docker-compose up -d

# Terminal 2: Start backend
cd back-end/node && cargo run

# Terminal 3: Start frontend
cd user-interface/flow-web && npm run dev
```

**Stop everything:**
```bash
npx nx stop-all
```

- [ ] Backend running at `http://localhost:8080`
- [ ] Frontend running at `http://localhost:3000`
- [ ] Docker services running (Qdrant, Redis)
- [ ] At least one space with indexed content exists
- [ ] Browser DevTools open (Network tab for monitoring API calls)

---

## Test Session Checklist

| Section | Status | Notes |
|---------|--------|-------|
| 1. Navigation & Layout | ⬜ | |
| 2. Search Page - Basic | ⬜ | |
| 3. Search Page - Scopes | ⬜ | |
| 4. Search - Keyboard Shortcuts | ⬜ | |
| 5. Search Results Interaction | ⬜ | |
| 6. Network Indicator | ⬜ | |
| 7. Space - Sync Status | ⬜ | |
| 8. Space - Publishing | ⬜ | |
| 9. Error Handling | ⬜ | |
| 10. Cross-Feature Integration | ⬜ | |

---

## 1. Navigation & Layout

### 1.1 Header Navigation
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 1.1.1 | Open app at `http://localhost:3000` | App loads, header visible | ⬜ |
| 1.1.2 | Look for "Search" link in header | Search navigation link is present | ⬜ |
| 1.1.3 | Look for Network Indicator in header | Network status indicator visible (green/yellow/red dot) | ⬜ |
| 1.1.4 | Click "Search" link | Navigates to `/search` page | ⬜ |

### 1.2 Responsive Header
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 1.2.1 | Resize browser to narrow width | Header elements remain accessible | ⬜ |
| 1.2.2 | Resize back to normal width | Layout restores properly | ⬜ |

---

## 2. Search Page - Basic Functionality

### 2.1 Initial State
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 2.1.1 | Navigate to `/search` | Search page loads with empty search bar | ⬜ |
| 2.1.2 | Observe search input | Placeholder text: "Search your knowledge..." or similar | ⬜ |
| 2.1.3 | Observe scope selector | Dropdown/toggle visible, default is "All" | ⬜ |
| 2.1.4 | Observe results area | Empty state or prompt to search | ⬜ |

### 2.2 Basic Search
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 2.2.1 | Type "test" in search bar | Input accepts text, no immediate search | ⬜ |
| 2.2.2 | Wait 300ms (debounce) | Search triggers automatically after debounce | ⬜ |
| 2.2.3 | Observe loading state | Loading indicator appears during search | ⬜ |
| 2.2.4 | Observe results | Results display with cards showing title, snippet, source | ⬜ |
| 2.2.5 | Check result count | "Found X results (Y local, Z network) in Nms" displayed | ⬜ |

### 2.3 Search with No Results
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 2.3.1 | Search for "xyznonexistent123" | Search completes | ⬜ |
| 2.3.2 | Observe empty state | Friendly message like "No results found" | ⬜ |
| 2.3.3 | Check for suggestions | Suggestions shown (try broader terms, adjust scope) | ⬜ |

### 2.4 Clear Search
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 2.4.1 | With results showing, clear search input | Results clear, returns to initial state | ⬜ |
| 2.4.2 | Type new search term | New search executes properly | ⬜ |

---

## 3. Search Page - Scope Selection

### 3.1 Scope Selector
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 3.1.1 | Click scope selector dropdown | Options appear: All, Local, Network | ⬜ |
| 3.1.2 | Select "Local" | Dropdown shows "Local" selected | ⬜ |
| 3.1.3 | Perform search | Only local results returned (blue badges) | ⬜ |
| 3.1.4 | Check API call (DevTools) | Request includes `scope: "local"` | ⬜ |

### 3.2 Network-Only Search
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 3.2.1 | Select "Network" scope | Dropdown shows "Network" selected | ⬜ |
| 3.2.2 | Perform search | Only network results returned (green badges) | ⬜ |
| 3.2.3 | If no peers connected | Appropriate message (no network results) | ⬜ |

### 3.3 All Scope (Combined)
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 3.3.1 | Select "All" scope | Dropdown shows "All" selected | ⬜ |
| 3.3.2 | Perform search | Both local and network results mixed | ⬜ |
| 3.3.3 | Results sorted by score | Higher scores appear first regardless of source | ⬜ |

### 3.4 Scope Persistence
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 3.4.1 | Set scope to "Local" | Scope is "Local" | ⬜ |
| 3.4.2 | Navigate to Home page | Page changes | ⬜ |
| 3.4.3 | Navigate back to Search | Scope remains "Local" (session persistence) | ⬜ |

---

## 4. Search - Keyboard Shortcuts

### 4.1 Global Search Shortcut
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 4.1.1 | From Home page, press `Cmd+K` (Mac) or `Ctrl+K` (Win) | Navigates to Search page | ⬜ |
| 4.1.2 | Search input is focused | Cursor in search bar, ready to type | ⬜ |

### 4.2 Shortcut from Any Page
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 4.2.1 | Navigate to Settings page | Settings page loads | ⬜ |
| 4.2.2 | Press `Cmd+K` | Navigates to Search, input focused | ⬜ |
| 4.2.3 | Navigate to a Space page | Space page loads | ⬜ |
| 4.2.4 | Press `Cmd+K` | Navigates to Search, input focused | ⬜ |

### 4.3 Search Results Keyboard Navigation
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 4.3.1 | Perform a search with multiple results | Results display | ⬜ |
| 4.3.2 | Press `Tab` from search input | Focus moves to first result | ⬜ |
| 4.3.3 | Press `Down Arrow` | Focus moves to next result | ⬜ |
| 4.3.4 | Press `Up Arrow` | Focus moves to previous result | ⬜ |
| 4.3.5 | Press `Enter` on focused result | Result action triggers (navigate or preview) | ⬜ |

---

## 5. Search Results Interaction

### 5.1 Local Result Click
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 5.1.1 | Perform search returning local results | Local results show (blue badge) | ⬜ |
| 5.1.2 | Click on a local result | Navigates to source space | ⬜ |
| 5.1.3 | Verify space page loads | Space details visible | ⬜ |

### 5.2 Network Result Click
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 5.2.1 | Perform search with network results | Network results show (green badge) | ⬜ |
| 5.2.2 | Click on a network result | Preview panel opens (not navigation) | ⬜ |
| 5.2.3 | Preview shows full snippet | Complete content snippet visible | ⬜ |
| 5.2.4 | Preview shows metadata | Peer ID, published date visible | ⬜ |
| 5.2.5 | "Copy CID" button works | CID copied to clipboard, feedback shown | ⬜ |
| 5.2.6 | Close preview | Preview closes, back to results | ⬜ |

### 5.3 Result Card Details
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 5.3.1 | Observe result card | Title displayed prominently | ⬜ |
| 5.3.2 | Observe snippet | Truncated preview of content | ⬜ |
| 5.3.3 | Observe score | Relevance score displayed (percentage) | ⬜ |
| 5.3.4 | Observe source badge | "LOCAL" or "NETWORK" badge with color | ⬜ |

---

## 6. Network Indicator

### 6.1 Connected State
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 6.1.1 | With backend running and peers connected | Green indicator visible | ⬜ |
| 6.1.2 | Hover over indicator | Tooltip shows peer count | ⬜ |
| 6.1.3 | Click indicator | Network details modal/panel opens | ⬜ |
| 6.1.4 | Details show peer list | Connected peers listed with IDs | ⬜ |
| 6.1.5 | Close details | Modal closes | ⬜ |

### 6.2 Disconnected State
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 6.2.1 | Stop backend (Ctrl+C in terminal) | Backend stops | ⬜ |
| 6.2.2 | Wait for status poll (30s) or refresh | Indicator changes to red/disconnected | ⬜ |
| 6.2.3 | Hover shows "Disconnected" | Tooltip indicates no connection | ⬜ |
| 6.2.4 | Start backend again | Backend runs | ⬜ |
| 6.2.5 | Wait for poll or refresh | Indicator returns to green | ⬜ |

### 6.3 No Peers State
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 6.3.1 | With backend running but no peers | Yellow/amber indicator | ⬜ |
| 6.3.2 | Tooltip shows "0 peers" | Status explains situation | ⬜ |

---

## 7. Space - Sync Status

### 7.1 View Sync Status
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 7.1.1 | Navigate to a space page | Space page loads | ⬜ |
| 7.1.2 | Locate sync status section | Status indicator visible | ⬜ |
| 7.1.3 | Observe file count | "X files indexed" displayed | ⬜ |
| 7.1.4 | Observe last sync time | "Last sync: X ago" displayed | ⬜ |

### 7.2 Trigger Reindex
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 7.2.1 | Find "Reindex" or "Sync" button | Button visible on space page | ⬜ |
| 7.2.2 | Click reindex button | Indexing starts | ⬜ |
| 7.2.3 | Observe progress indicator | Progress bar or spinner shows | ⬜ |
| 7.2.4 | Wait for completion | Status updates to "Sync complete" | ⬜ |
| 7.2.5 | Verify updated timestamp | Last sync time is now | ⬜ |

### 7.3 Indexing Progress (During Reindex)
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 7.3.1 | During active indexing | Progress percentage or file count shown | ⬜ |
| 7.3.2 | Status text updates | "Indexing in progress..." or similar | ⬜ |
| 7.3.3 | Polling is faster during indexing | Status updates every ~5 seconds | ⬜ |

---

## 8. Space - Publishing

### 8.1 Unpublished Space
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 8.1.1 | Navigate to an unpublished space | Space page loads | ⬜ |
| 8.1.2 | Locate "Publish" button | Button visible and enabled | ⬜ |
| 8.1.3 | Verify "Private" or no badge | Space shows as not published | ⬜ |

### 8.2 Publish Flow
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 8.2.1 | Click "Publish" button | Confirmation modal opens | ⬜ |
| 8.2.2 | Read modal content | Explains what publishing does | ⬜ |
| 8.2.3 | Verify checkbox present | "I understand" checkbox unchecked | ⬜ |
| 8.2.4 | Publish button is disabled | Cannot publish without checkbox | ⬜ |
| 8.2.5 | Check the checkbox | Checkbox becomes checked | ⬜ |
| 8.2.6 | Publish button enables | Button now clickable | ⬜ |
| 8.2.7 | Click "Publish" | Loading state shown | ⬜ |
| 8.2.8 | Wait for completion | Success message shown | ⬜ |
| 8.2.9 | Modal closes | Returns to space page | ⬜ |
| 8.2.10 | Verify published state | "Published" badge/status shown | ⬜ |
| 8.2.11 | Verify published timestamp | "Published X ago" displayed | ⬜ |

### 8.3 Published Space
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 8.3.1 | Navigate to a published space | Space page loads | ⬜ |
| 8.3.2 | Verify "Published" indicator | Green badge or checkmark visible | ⬜ |
| 8.3.3 | Button shows published state | "Published ✓" or dropdown | ⬜ |

### 8.4 Unpublish Flow
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 8.4.1 | On published space, click published button | Dropdown or options appear | ⬜ |
| 8.4.2 | Select "Unpublish" option | Confirmation dialog appears | ⬜ |
| 8.4.3 | Confirm unpublish | Loading state | ⬜ |
| 8.4.4 | Wait for completion | Success message | ⬜ |
| 8.4.5 | Verify unpublished state | "Publish" button returns | ⬜ |

### 8.5 Cancel Actions
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 8.5.1 | Open publish modal | Modal opens | ⬜ |
| 8.5.2 | Click "Cancel" | Modal closes, no action taken | ⬜ |
| 8.5.3 | Space remains unpublished | State unchanged | ⬜ |

---

## 9. Error Handling

### 9.1 Search Errors
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 9.1.1 | Stop backend, try search | Error message displayed | ⬜ |
| 9.1.2 | Error has retry option | "Retry" button or suggestion | ⬜ |
| 9.1.3 | Start backend, retry | Search works after recovery | ⬜ |

### 9.2 Publish Errors
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 9.2.1 | (If testable) Trigger publish failure | Error message in modal | ⬜ |
| 9.2.2 | Error explains issue | Clear error message | ⬜ |
| 9.2.3 | Can dismiss and retry | Modal closeable, retry possible | ⬜ |

### 9.3 Network Status Errors
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 9.3.1 | With backend stopped | Indicator shows disconnected | ⬜ |
| 9.3.2 | Click indicator | Shows appropriate error/offline state | ⬜ |

---

## 10. Cross-Feature Integration

### 10.1 End-to-End: Publish and Search
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 10.1.1 | Navigate to an unpublished, indexed space | Space page loads | ⬜ |
| 10.1.2 | Publish the space | Publication succeeds | ⬜ |
| 10.1.3 | Navigate to Search | Search page loads | ⬜ |
| 10.1.4 | Set scope to "Local" | Scope changed | ⬜ |
| 10.1.5 | Search for content from that space | Content appears in local results | ⬜ |

### 10.2 End-to-End: Network Search
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 10.2.1 | Verify network indicator shows peers | At least 1 peer connected | ⬜ |
| 10.2.2 | Navigate to Search | Search page loads | ⬜ |
| 10.2.3 | Set scope to "All" | Scope is "All" | ⬜ |
| 10.2.4 | Search for a term | Results include network results | ⬜ |
| 10.2.5 | Click network result | Preview opens correctly | ⬜ |

### 10.3 End-to-End: Reindex and Search
| # | Action | Expected Result | Pass/Fail |
|---|--------|-----------------|-----------|
| 10.3.1 | Navigate to a space | Space page loads | ⬜ |
| 10.3.2 | Trigger reindex | Indexing starts | ⬜ |
| 10.3.3 | Wait for completion | Indexing completes | ⬜ |
| 10.3.4 | Navigate to Search | Search page loads | ⬜ |
| 10.3.5 | Search for space content | Results found | ⬜ |

---

## Test Session Summary

**Date:** ________________
**Tester:** ________________
**Total Tests:** 100+
**Passed:** ____
**Failed:** ____
**Blocked:** ____

### Issues Found

| # | Severity | Description | Steps to Reproduce |
|---|----------|-------------|-------------------|
| 1 | | | |
| 2 | | | |
| 3 | | | |

### Notes

```
[Additional observations, suggestions, or notes]
```

---

## Quick Reference: API Endpoints Used

| Feature | Endpoint | Method |
|---------|----------|--------|
| Search | `/api/v1/search/distributed` | POST |
| Network Status | `/api/v1/network/status` | GET |
| Space Status | `/api/v1/spaces/{key}/status` | GET |
| Reindex | `/api/v1/spaces/{key}/reindex` | POST |
| Publish | `/api/v1/spaces/{key}/publish` | POST |
| Unpublish | `/api/v1/spaces/{key}/publish` | DELETE |

---

## Quick Start Commands

```bash
# From project root: /Users/julian/Documents/Code/Flow Network/Flow

# Start everything (recommended - one command)
npx nx start-all

# Stop everything
npx nx stop-all

# Individual commands (if needed)
npx nx run back-end:docker-up     # Start Qdrant + Redis
npx nx run back-end:docker-down   # Stop Docker services
npx nx run back-end:docker-logs   # View Docker logs
npx nx run back-end:run-node      # Start Rust backend only
npx nx run user-interface:dev-web # Start React frontend only

# Run tests
npx nx run user-interface:test    # Frontend tests
npx nx run back-end:test          # Backend tests
```
