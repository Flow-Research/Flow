# Product Specification: Codebase Simplification & UI Fixes

**Version:** 1.1
**Date:** 2026-01-09
**Status:** Reviewed
**Author:** Product Team
**Reviewed:** 5-Perspective PRD Review

---

## 1. Executive Summary

### Product Name
Flow Codebase Health & UI Polish

### Vision Statement
Reduce technical complexity in the Flow backend while improving the user experience on the "Your Spaces" page, preparing the codebase for the upcoming P2P networking roadmap.

### Problem Statement
1. **Backend Complexity**: The codebase has grown organically and may contain over-engineered patterns that slow iteration and increase cognitive load.
2. **Poor Space Identification**: Users see cryptographic space IDs (64-character hashes) instead of human-readable names, making space management confusing.
3. **UI Overflow Issues**: Space cards on the home page have content that overflows boundaries, creating visual inconsistency.

### Target Users
- Primary developers maintaining the Flow codebase
- End users managing their document spaces
- Future contributors who need to understand the codebase

### Key Outcomes
- Prioritized list of simplification opportunities with effort estimates
- Space cards display human-readable names
- All card content properly contained within boundaries

---

## 2. Goals and Objectives

### Business Objectives

| Objective | Metric | Target |
|-----------|--------|--------|
| Reduce codebase complexity | Lines of code removed or simplified | 10-20% reduction in identified modules |
| Improve developer velocity | Time to implement new features | Baseline + qualitative assessment |
| Improve user comprehension | User can identify spaces by name | 100% of spaces show readable names |
| Fix UI regressions | Visual bugs on Your Spaces page | 0 overflow issues |

### Product Objectives

| Objective | Description |
|-----------|-------------|
| **OBJ-1** | Complete comprehensive backend audit with actionable findings |
| **OBJ-2** | Add space naming capability to data model and UI |
| **OBJ-3** | Fix card layout truncation issues |
| **OBJ-4** | Document findings for future reference |

### Non-Goals

- Performance optimization (unless directly related to over-engineering)
- New feature development beyond space naming
- Frontend architecture changes beyond the specified UI fixes
- Refactoring that would delay the P2P networking roadmap

---

## 3. User Personas

### Persona 1: Solo Developer (Primary)

| Attribute | Description |
|-----------|-------------|
| **Name** | Julian |
| **Role** | Primary developer and maintainer |
| **Goals** | Ship features faster, reduce mental overhead |
| **Pain Points** | Gut feeling that code is heavier than needed; hard to remember which space is which |
| **Technical Level** | Expert |

### Persona 2: Future Contributor

| Attribute | Description |
|-----------|-------------|
| **Name** | Alex |
| **Role** | Open source contributor |
| **Goals** | Understand codebase quickly, make meaningful contributions |
| **Pain Points** | Complex abstractions slow onboarding; unclear naming conventions |
| **Technical Level** | Intermediate to Advanced |

### Persona 3: End User

| Attribute | Description |
|-----------|-------------|
| **Name** | Sam |
| **Role** | Knowledge worker using Flow for document management |
| **Goals** | Quickly find and access document spaces |
| **Pain Points** | Can't tell spaces apart when they all show cryptographic IDs |
| **Technical Level** | Non-technical |

---

## 4. User Stories and Requirements

### Epic 1: Backend Audit

#### User Stories

| ID | Story | Priority | Acceptance Criteria |
|----|-------|----------|---------------------|
| **US-1.1** | As a developer, I want to see a categorized list of over-engineering findings so I can prioritize simplification work | P0 | Findings categorized as High/Medium/Low priority |
| **US-1.2** | As a developer, I want each finding to include a recommendation and effort estimate so I can plan work | P0 | Each finding has: what, why, recommendation, effort |
| **US-1.3** | As a developer, I want findings scoped to specific files/modules so I can act on them | P0 | Each finding references specific file paths and line numbers |

#### Functional Requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| **FR-1.1** | Audit shall cover: modules/, bootstrap/, api/, entities/ directories | P0 |
| **FR-1.2** | Audit shall evaluate: traits/abstractions, error handling, configuration complexity | P0 |
| **FR-1.3** | Audit shall produce a structured report in markdown format | P0 |
| **FR-1.4** | Findings shall be actionable without breaking existing functionality | P0 |

### Epic 2: Space Naming

#### User Stories

| ID | Story | Priority | Acceptance Criteria |
|----|-------|----------|---------------------|
| **US-2.1** | As a user, I want to see a readable name for each space so I can identify them quickly | P0 | Space cards show name instead of hash |
| **US-2.2** | As a user, I want the space name derived from my folder name by default so I don't have to enter it manually | P1 | Default name = last path segment |
| **US-2.3** | As a user, I want to rename a space if the default name isn't helpful | P2 | [FUTURE] Rename UI available |

#### Functional Requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| **FR-2.1** | Space entity shall include a `name` field (VARCHAR, nullable) | P0 |
| **FR-2.2** | When `name` is null or empty, UI shall derive display name from `location` path (last segment) | P0 |
| **FR-2.3** | Space creation API shall auto-populate `name` from path if not provided (backend derives, not stored as null) | P1 |
| **FR-2.4** | Database migration shall backfill `name` for existing spaces | P1 |
| **FR-2.5** | If derived name would duplicate an existing space name, append location hash suffix (e.g., "docs-7f3a") | P1 |

**Clarification on FR-2.2 vs FR-2.3:**
- FR-2.3 ensures new spaces get a name at creation time (stored in DB)
- FR-2.2 is a fallback for edge cases (migration failures, manual DB edits, legacy data)
- In normal operation, name will always be populated; UI derivation is defensive

### Epic 3: Card Truncation Fix

#### User Stories

| ID | Story | Priority | Acceptance Criteria |
|----|-------|----------|---------------------|
| **US-3.1** | As a user, I want space cards to have consistent sizing so the page looks professional | P0 | All cards same height in each row |
| **US-3.2** | As a user, I want long names/paths to truncate with ellipsis so they don't break the layout | P0 | Text truncates, shows "..." on overflow |
| **US-3.3** | As a user, I want to see full details on hover so I can read truncated text | P1 | Tooltip shows full text on hover |

#### Functional Requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| **FR-3.1** | Space name (h3) shall truncate with ellipsis after 1 line | P0 |
| **FR-3.2** | Space location shall truncate with ellipsis after 1 line | P0 |
| **FR-3.3** | Card container shall have `overflow: hidden` | P0 |
| **FR-3.4** | Truncated text shall show full content in `title` attribute (tooltip) | P1 |

---

## 5. Functional Specifications

### 5.1 Backend Audit Process

#### Audit Type
**Manual code review** with structured evaluation criteria. This is a human-driven analysis, not automated tooling.

#### Audit Methodology

```
1. Module Inventory (Manual + tokei/cloc for metrics)
   └── List all modules in back-end/node/src/
   └── Count files, lines, public exports per module
   └── Tool: `tokei back-end/node/src/` for line counts

2. Pattern Analysis (Manual Review)
   └── Identify traits with single implementations (candidate for removal)
   └── Find wrapper types that add no functionality
   └── Detect configuration complexity vs. actual usage
   └── Review error types granularity
   └── Look for: excessive generics, premature abstraction, unused flexibility

3. Dependency Mapping (Manual + cargo-deps)
   └── Graph inter-module dependencies
   └── Identify circular or excessive coupling
   └── Tool: `cargo deps --all-features` for visualization

4. Scoring & Prioritization
   └── Impact: How much does this slow iteration? (1-3)
   └── Effort: How hard to simplify? (1-3)
   └── Priority = Impact / Effort (higher = fix first)
   └── Categorize: High (>1.5), Medium (1.0-1.5), Low (<1.0)

5. Documentation
   └── Generate audit_report.md with findings
   └── Include before/after code examples where helpful
   └── Each finding format: [Module] [Issue] [Why] [Recommendation] [Effort]
```

#### Audit Scope

| Directory | Focus Areas |
|-----------|-------------|
| `modules/ai/` | Pipeline complexity, trait usage, config handling |
| `modules/network/` | Manager abstraction, event handling patterns |
| `modules/storage/` | KV store abstraction, content addressing types |
| `modules/query/` | RAG pipeline, search abstractions |
| `modules/knowledge/` | Entity extraction, type definitions |
| `bootstrap/` | Configuration loading, initialization flow |
| `api/` | Handler organization, error responses |

### 5.2 Space Naming Feature

#### Data Model Change

```sql
-- Migration: Add name column to space table (SQLite compatible)
ALTER TABLE space ADD COLUMN name VARCHAR(255);

-- Backfill existing spaces with derived names (SQLite compatible)
-- Extracts last path segment: "/Users/julian/Documents/specs" → "specs"
UPDATE space
SET name = CASE
  -- Handle trailing slash: "/path/to/folder/" → "folder"
  WHEN SUBSTR(location, -1) = '/' THEN
    SUBSTR(
      RTRIM(location, '/'),
      LENGTH(RTRIM(location, '/')) - LENGTH(
        REPLACE(RTRIM(location, '/'), RTRIM(REPLACE(RTRIM(location, '/'), '/', ''), '/'), '')
      ) + 1
    )
  -- Normal case: "/path/to/folder" → "folder"
  ELSE
    SUBSTR(location, LENGTH(location) - LENGTH(REPLACE(location, RTRIM(REPLACE(location, '/', ''), '/'), '')) + 1)
  END
WHERE name IS NULL;

-- Alternative: Use application-level backfill for complex path parsing
-- Run: SELECT id, location FROM space WHERE name IS NULL;
-- Then update each row with application code that properly parses paths
```

#### Display Logic

```
IF space.name IS NOT NULL AND space.name != ''
  THEN display space.name
ELSE
  THEN derive from space.location (last path segment)
```

#### API Changes

| Endpoint | Change |
|----------|--------|
| `GET /api/v1/spaces` | Response includes `name` field |
| `GET /api/v1/spaces/:key` | Response includes `name` field |
| `POST /api/v1/spaces` | Accepts optional `name` in request body |
| `PATCH /api/v1/spaces/:key` | [FUTURE] Update `name` field |

### 5.3 Card Truncation Fix

#### CSS Changes

```css
/* Space card container */
.space-card {
  overflow: hidden;  /* Contain all overflow */
}

/* Space name - single line with ellipsis */
.space-info h3 {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

/* Space location - single line with ellipsis */
.space-location {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  word-break: normal;  /* Override break-all */
}
```

#### Component Changes

```tsx
// Add title attributes for tooltips
<h3 title={spaceName}>{spaceName}</h3>
<p className="space-location" title={space.location}>
  {space.location}
</p>
```

---

## 6. Non-Functional Requirements

### Performance

| Requirement | Target |
|-------------|--------|
| Audit execution time | Complete within 1 working day |
| Space name derivation | < 1ms per space |
| UI render time | No regression from current baseline |

### Security

| Requirement | Description |
|-------------|-------------|
| Input validation | Space names sanitized (no XSS vectors) |
| Path exposure | Location paths only shown to authenticated users |

### Maintainability

| Requirement | Description |
|-------------|-------------|
| Audit documentation | Findings preserved in specs/ for future reference |
| Code comments | Simplified code should be self-documenting |

### Compatibility

| Requirement | Description |
|-------------|-------------|
| Database migration | Backwards compatible (nullable column) |
| API versioning | No breaking changes to existing endpoints |
| Browser support | Chrome, Firefox, Safari (latest 2 versions) |

---

## 7. User Interface Specifications

### 7.1 Your Spaces Page - Current State

```
┌─────────────────────────────────────────────────────────┐
│  Your Spaces                           [+ Create Space] │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌──────────────────┐  ┌──────────────────┐            │
│  │ 📂               │  │ 📂               │            │
│  │ 79c50f2e702c3e56 │  │ a4c6456c8cd497fe │            │
│  │ 535062323a691... │  │ c0e8492a40e323...│  ← PROBLEM │
│  │ /Users/julian/Do │  │ /very/long/path/ │  ← OVERFLOW│
│  │ Created 1/8/2026 │  │ Created 1/5/2026 │            │
│  └──────────────────┘  └──────────────────┘            │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 7.2 Your Spaces Page - Target State

```
┌─────────────────────────────────────────────────────────┐
│  Your Spaces                           [+ Create Space] │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌──────────────────┐  ┌──────────────────┐            │
│  │ 📂               │  │ 📂               │            │
│  │ specs            │  │ project-docs     │  ← NAME    │
│  │ /Users/julian... │  │ /very/long/pa... │  ← ELLIPSIS│
│  │ Created 1/8/2026 │  │ Created 1/5/2026 │            │
│  └──────────────────┘  └──────────────────┘            │
│                                                         │
│  [Hover shows full path in tooltip]                     │
└─────────────────────────────────────────────────────────┘
```

### 7.3 Visual Hierarchy

| Element | Treatment |
|---------|-----------|
| Space Name | Primary text, bold, single line, truncate with ellipsis |
| Location Path | Secondary text, monospace, muted color, truncate |
| Created Date | Tertiary text, smallest size, muted |
| Card | Fixed max-width, consistent padding, overflow hidden |

---

## 8. Technical Architecture

### 8.1 Affected Components

```
┌─────────────────────────────────────────────────────────────┐
│                      BACKEND CHANGES                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐     ┌─────────────────┐               │
│  │ entity/space.rs │────▶│  Migration      │               │
│  │ + name: Option  │     │  + ADD COLUMN   │               │
│  └─────────────────┘     └─────────────────┘               │
│           │                                                 │
│           ▼                                                 │
│  ┌─────────────────┐                                       │
│  │ api/spaces.rs   │                                       │
│  │ + return name   │                                       │
│  └─────────────────┘                                       │
│                                                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                     FRONTEND CHANGES                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐     ┌─────────────────┐               │
│  │ services/api.ts │────▶│  Home.tsx       │               │
│  │ + Space.name    │     │  + display name │               │
│  └─────────────────┘     │  + truncation   │               │
│                          └─────────────────┘               │
│                                   │                         │
│                                   ▼                         │
│                          ┌─────────────────┐               │
│                          │  Home.css       │               │
│                          │  + ellipsis     │               │
│                          │  + overflow     │               │
│                          └─────────────────┘               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 8.2 Database Schema Change

**Current Schema:**
```sql
CREATE TABLE space (
    id INTEGER PRIMARY KEY,
    key VARCHAR(64) NOT NULL UNIQUE,
    location VARCHAR(512) NOT NULL,
    time_created TIMESTAMP WITH TIME ZONE NOT NULL
);
```

**Target Schema:**
```sql
CREATE TABLE space (
    id INTEGER PRIMARY KEY,
    key VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(255),  -- NEW: Human-readable name
    location VARCHAR(512) NOT NULL,
    time_created TIMESTAMP WITH TIME ZONE NOT NULL
);
```

---

## 9. Integration Points

### 9.1 Internal Integrations

| System | Integration Type | Changes Required |
|--------|------------------|------------------|
| Sea-ORM Entity | Data Model | Add `name` field to Space model |
| REST API | Response Schema | Include `name` in space responses |
| React Frontend | API Client | Update Space type definition |
| Space Module | Business Logic | Derive name from path if null |

### 9.2 External Integrations

None. This is an internal improvement with no external dependencies.

---

## 10. Data Requirements

### 10.1 Data Model Changes

| Entity | Field | Type | Nullable | Default | Description |
|--------|-------|------|----------|---------|-------------|
| Space | name | VARCHAR(255) | Yes | NULL | Human-readable space name |

### 10.2 Migration Strategy

1. **Add Column**: `ALTER TABLE space ADD COLUMN name VARCHAR(255)`
2. **Backfill**: Derive names from location paths for existing spaces
3. **Application Update**: Deploy code that uses new field
4. **Verification**: Confirm all spaces display names correctly

### 10.3 Data Validation

| Field | Validation |
|-------|------------|
| name | Max 255 chars, trimmed whitespace, sanitized for XSS |

---

## 11. Analytics and Metrics

### 11.1 Audit Metrics

| Metric | Description | Target |
|--------|-------------|--------|
| Findings count | Total issues identified | Document all |
| High priority count | Critical simplification opportunities | Track for prioritization |
| Estimated effort | Total hours to implement all recommendations | Informational |

### 11.2 UI Metrics

| Metric | Description | Target |
|--------|-------------|--------|
| Visual regressions | UI bugs introduced | 0 |
| Truncation issues | Content overflow after fix | 0 |

---

## 12. Security Considerations

### 12.1 Threat Analysis

| Threat | Risk | Mitigation |
|--------|------|------------|
| XSS via space name | Low | Sanitize input, React escapes by default |
| Path traversal | N/A | Names are display-only, not used for file access |
| Information disclosure | Low | Paths only shown to authenticated users |

### 12.2 Security Requirements

- Space names must be sanitized before storage
- No executable content in name field
- Audit findings should not be committed with sensitive data

---

## 13. Testing Strategy

### 13.1 Backend Testing

| Test Type | Scope | Coverage Target |
|-----------|-------|-----------------|
| Unit Tests | Space entity, name derivation logic | 100% |
| Integration Tests | API endpoints return name field | All endpoints |
| Migration Tests | Backfill works correctly | Verify on test data |

#### Edge Case Test Cases

| Test Case | Input | Expected Output |
|-----------|-------|-----------------|
| Normal path | `/Users/julian/Documents/specs` | `specs` |
| Trailing slash | `/Users/julian/Documents/specs/` | `specs` |
| Root path | `/` | `/` (or empty handling) |
| Single segment | `docs` | `docs` |
| Special characters | `/path/to/my docs (2024)` | `my docs (2024)` |
| Unicode | `/path/to/文档` | `文档` |
| Duplicate name | Two folders named `docs` | `docs`, `docs-7f3a` |
| Empty after trim | `/path/to/   /` | Fallback to hash prefix |

### 13.2 Frontend Testing

| Test Type | Scope | Coverage Target |
|-----------|-------|-----------------|
| Unit Tests | Name display logic, truncation | Component level |
| Visual Tests | Card layout, ellipsis rendering | Manual verification |
| Browser Tests | Cross-browser truncation behavior | Chrome, Firefox, Safari |

### 13.3 Audit Validation

| Validation | Description |
|------------|-------------|
| Reproducibility | Another developer can verify findings |
| Non-regression | Existing tests pass after any simplification |

---

## 14. Launch Plan

### 14.1 Phases

| Phase | Deliverables | Duration |
|-------|--------------|----------|
| **Phase 1: Audit** | Audit report with findings | 1 day |
| **Phase 2: Database** | Migration, entity update, API changes | 0.5 day |
| **Phase 3: Frontend** | UI updates, CSS fixes | 0.5 day |
| **Phase 4: Testing** | Verify all changes, fix issues | 0.5 day |
| **Phase 5: Documentation** | Update any affected docs | 0.25 day |

### 14.2 Rollout Strategy

1. **Development**: Implement all changes on feature branch
2. **Testing**: Run full test suite, manual UI verification
3. **Review**: Code review for all changes
4. **Merge**: Merge to main branch
5. **Deploy**: Standard deployment process

### 14.3 Rollback Plan

- Database migration is additive (new nullable column) - no rollback needed
- Frontend changes are isolated to Home.tsx and Home.css
- If issues arise, revert frontend commit

---

## 15. Risks and Mitigations

### 15.1 Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Audit finds nothing significant | Low | Low | Document current state as healthy baseline |
| Migration fails on production data | Low | Medium | Test migration on copy of production DB |
| CSS changes break other pages | Low | Medium | Scope CSS selectors carefully, test thoroughly |
| Name derivation produces poor defaults | Medium | Low | Allow user override in future iteration |

### 15.2 Dependencies

| Dependency | Risk | Mitigation |
|------------|------|------------|
| Sea-ORM migration system | Low | Well-established pattern in codebase |
| React/CSS support for text-overflow | None | Standard CSS property |

---

## 16. Success Criteria

### 16.1 Audit Success

- [ ] All backend modules reviewed
- [ ] Findings documented with priority, recommendation, and effort
- [ ] Report provides clear path for incremental improvements

### 16.2 UI Success

- [ ] Space cards show human-readable names
- [ ] No content overflows card boundaries
- [ ] Truncated text shows full content on hover
- [ ] Existing tests pass
- [ ] No visual regressions on other pages

### 16.3 Overall Success

- [ ] Developer reports improved clarity about simplification opportunities
- [ ] User can identify spaces at a glance
- [ ] Codebase ready for P2P networking roadmap work

---

## Appendix A: Current Code Analysis

### A.1 Space Entity (Current)

**File:** `back-end/entity/src/space.rs`

```rust
pub struct Model {
    pub id: i32,
    pub key: String,        // 64-char hash
    pub location: String,   // Full filesystem path
    pub time_created: DateTimeWithTimeZone,
    // NOTE: No name field
}
```

### A.2 Home.tsx (Current Issue)

**File:** `user-interface/flow-web/src/pages/Home.tsx`

```tsx
// Line 85: Shows cryptographic key instead of name
<h3>{space.key}</h3>

// Line 86: Location can overflow
<p className="space-location">{space.location}</p>
```

### A.3 Home.css (Current Issue)

**File:** `user-interface/flow-web/src/pages/Home.css`

```css
/* Line 106-110: No truncation on h3 */
.space-info h3 {
  margin: 0 0 0.5rem;
  font-size: 1.125rem;
  color: #fff;
  /* Missing: white-space, overflow, text-overflow */
}

/* Line 112-118: word-break causes wrap but no truncation */
.space-location {
  word-break: break-all;  /* Causes wrapping, not truncation */
  /* Missing: white-space, overflow, text-overflow */
}
```

---

## Appendix B: Glossary

| Term | Definition |
|------|------------|
| Space | A collection of documents from a filesystem directory |
| Space Key | 64-character SHA-256 hash identifying a space |
| Space Name | Human-readable identifier for a space |
| Over-engineering | Code complexity beyond what's needed for current requirements |
| Truncation | Cutting off text with "..." when it exceeds available space |

---

## Appendix C: References

- Brainstorm: `specs/02_simplification_ui/brainstorm.md`
- Backend Source: `back-end/node/src/`
- Frontend Source: `user-interface/flow-web/src/`
- Flow Roadmap: P2P Networking Implementation (Phases 0-5)
