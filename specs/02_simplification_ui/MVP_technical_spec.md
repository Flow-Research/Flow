# MVP Technical Specification: Codebase Simplification & UI Fixes

**Version:** 1.0
**Date:** 2026-01-10
**Status:** Ready for Implementation
**Source:** specs/02_simplification_ui/technical_spec.md v1.1

---

## 1. MVP Overview

### 1.1 MVP Hypothesis

**If** we add human-readable space names and fix card truncation, **then** users will be able to identify and manage their spaces more efficiently, **validated by** 100% of spaces showing readable names with no visual overflow.

### 1.2 MVP Scope Summary

| Epic | MVP Scope | Deferred |
|------|-----------|----------|
| **Backend Audit** | Full audit with documented findings | None |
| **Space Naming** | Name field, auto-derivation, backfill migration | Rename UI (future) |
| **Card Truncation** | Ellipsis truncation, tooltips | None |

### 1.3 MVP Success Criteria

- [ ] Audit report produced with prioritized findings
- [ ] Space cards display human-readable names (not hashes)
- [ ] All text truncates with ellipsis, no overflow
- [ ] Tooltips show full text on hover
- [ ] All existing tests pass
- [ ] No breaking API changes

---

## 2. MVP Technical Decisions

### 2.1 Data Model

| Decision | MVP Choice | Future-Proof? | Rationale |
|----------|------------|---------------|-----------|
| Name storage | Nullable VARCHAR(255) | Yes | Supports future rename feature |
| Name derivation | Backend at creation | Yes | Consistent, no UI logic needed |
| Duplicate handling | Hash suffix | Yes | Simple, deterministic |

### 2.2 What's Deferred

| Feature | Why Deferred | Migration Path |
|---------|--------------|----------------|
| Rename UI | Not MVP-essential | Add PATCH endpoint + UI in future |
| Custom name in API | Can use derived name | Signature already supports it |

---

## 3. MVP Architecture

### 3.1 Files to Modify

```
back-end/
├── entity/src/space.rs          # Add name field
├── migration/src/
│   ├── lib.rs                   # Register migration
│   └── m20260110_*.rs           # NEW: Add name column
└── node/src/
    ├── api/servers/rest.rs      # Include name in responses
    └── modules/space.rs         # Name derivation logic

user-interface/flow-web/src/
├── services/api.ts              # Add name to Space type
└── pages/
    ├── Home.tsx                 # Display name + tooltips
    └── Home.css                 # Truncation styles

specs/02_simplification_ui/
└── audit_report.md              # NEW: Audit findings
```

### 3.2 No New Dependencies

All required libraries already in project:
- `sha2` for hash fallback
- `sea-orm` for migrations
- Standard CSS for truncation

---

## 4. MVP Work Items

### 4.1 Prioritized Work Item List

| ID | Task | Priority | Effort | Dependencies | Acceptance Criteria |
|----|------|----------|--------|--------------|---------------------|
| **MVP-1** | Create database migration | P0 | 30m | None | Column added, backfill works |
| **MVP-2** | Register migration in lib.rs | P0 | 5m | MVP-1 | Migration runs with `cargo run -p migration -- up` |
| **MVP-3** | Update entity/space.rs | P0 | 10m | MVP-1 | Compiles, name field serializes |
| **MVP-4** | Update modules/space.rs | P0 | 45m | MVP-3 | Name derived on creation, duplicates handled |
| **MVP-5** | Update rest.rs handlers | P0 | 20m | MVP-4 | API returns name in responses |
| **MVP-6** | Update api.ts Space type | P0 | 5m | None | TypeScript interface includes name |
| **MVP-7** | Update Home.tsx | P0 | 20m | MVP-6 | Displays name, has title attributes |
| **MVP-8** | Update Home.css | P0 | 15m | None | Truncation with ellipsis works |
| **MVP-9** | Run backend unit tests | P1 | 15m | MVP-4 | All tests pass |
| **MVP-10** | Manual UI verification | P1 | 15m | MVP-7, MVP-8 | Visual confirmation |
| **MVP-11** | Conduct backend audit | P0 | 4h | None | Audit report produced |
| **MVP-12** | Document audit findings | P0 | 1h | MVP-11 | audit_report.md complete |

### 4.2 Total Effort Estimate

| Phase | Items | Effort |
|-------|-------|--------|
| Database & Backend | MVP-1 to MVP-5 | ~2 hours |
| Frontend | MVP-6 to MVP-8 | ~40 minutes |
| Testing | MVP-9 to MVP-10 | ~30 minutes |
| Audit | MVP-11 to MVP-12 | ~5 hours |
| **Total** | 12 items | **~8 hours** |

### 4.3 Implementation Sequence

```
┌─────────────────────────────────────────────────────────────────┐
│  PHASE 1: Database & Backend (Can start immediately)            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  MVP-1 ──► MVP-2 ──► MVP-3 ──► MVP-4 ──► MVP-5                 │
│  (migration) (register) (entity) (module) (API)                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│  PHASE 2: Frontend (Parallel after MVP-6 ready)                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  MVP-6 ──► MVP-7                 MVP-8 (can run parallel)       │
│  (types)   (component)           (CSS)                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
          │
          ▼
┌─────────────────────────────────────────────────────────────────┐
│  PHASE 3: Testing                                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  MVP-9 (unit tests) ──► MVP-10 (manual verification)            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  PHASE 4: Audit (Can run in parallel with all above)            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  MVP-11 (conduct audit) ──► MVP-12 (document findings)          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 5. Detailed Work Item Specifications

### MVP-1: Create Database Migration

**File:** `back-end/migration/src/m20260110_000000_add_space_name.rs`

**Implementation:**
```rust
use sea_orm_migration::prelude::*;
use sha2::{Digest, Sha256};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add nullable name column
        manager
            .alter_table(
                Table::alter()
                    .table(Space::Table)
                    .add_column(ColumnDef::new(Space::Name).string_len(255).null())
                    .to_owned(),
            )
            .await?;

        // Backfill existing spaces
        let db = manager.get_connection();
        let spaces: Vec<(i32, String)> = db
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id, location FROM space WHERE name IS NULL".to_owned(),
            ))
            .await?
            .iter()
            .map(|row| {
                let id: i32 = row.try_get("", "id").unwrap();
                let location: String = row.try_get("", "location").unwrap();
                (id, location)
            })
            .collect();

        for (id, location) in spaces {
            let name = derive_name_from_path(&location);
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "UPDATE space SET name = $1 WHERE id = $2",
                [name.into(), id.into()],
            ))
            .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Space::Table)
                    .drop_column(Space::Name)
                    .to_owned(),
            )
            .await
    }
}

fn derive_name_from_path(location: &str) -> String {
    let trimmed = location.trim_end_matches('/');
    let path = std::path::Path::new(trimmed);

    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let trimmed_name = name.trim();
        if !trimmed_name.is_empty() {
            return trimmed_name.to_owned();
        }
    }

    // Fallback: hash prefix
    let mut hasher = Sha256::new();
    hasher.update(location.as_bytes());
    let hash = hasher.finalize();
    format!("{:x}", hash)[..8].to_owned()
}

#[derive(DeriveIden)]
enum Space {
    Table,
    Name,
}
```

**Acceptance Criteria:**
- [ ] Migration file compiles
- [ ] `cargo run -p migration -- up` succeeds
- [ ] Existing spaces have names populated
- [ ] `cargo run -p migration -- down` removes column

---

### MVP-2: Register Migration

**File:** `back-end/migration/src/lib.rs`

**Change:**
```rust
mod m20260110_000000_add_space_name;

// Add to migrations() vec:
Box::new(m20260110_000000_add_space_name::Migration),
```

**Acceptance Criteria:**
- [ ] Migration appears in migration list
- [ ] Runs in correct order

---

### MVP-3: Update Entity

**File:** `back-end/entity/src/space.rs`

**Change:**
```rust
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub key: String,
    pub name: Option<String>,  // NEW
    pub location: String,
    pub time_created: DateTimeWithTimeZone,
}
```

**Acceptance Criteria:**
- [ ] Entity compiles
- [ ] Serializes to JSON with `name` field

---

### MVP-4: Update Space Module

**File:** `back-end/node/src/modules/space.rs`

**Changes:**
1. Update `new_space` signature to accept `custom_name: Option<&str>`
2. Add `derive_unique_name` function
3. Set name in ActiveModel

**Acceptance Criteria:**
- [ ] New spaces get derived names
- [ ] Duplicate names get hash suffix
- [ ] Compiles and tests pass

---

### MVP-5: Update REST Handlers

**File:** `back-end/node/src/api/servers/rest.rs`

**Changes:**
1. Include `name` in `list_spaces` response
2. Include `name` in `get_space` response

**Acceptance Criteria:**
- [ ] GET /api/v1/spaces returns `name` field
- [ ] GET /api/v1/spaces/:key returns `name` field

---

### MVP-6: Update Frontend Types

**File:** `user-interface/flow-web/src/services/api.ts`

**Change:**
```typescript
export interface Space {
  id: number;
  key: string;
  name: string | null;  // NEW
  location: string;
  time_created: string;
}
```

**Acceptance Criteria:**
- [ ] TypeScript compiles
- [ ] No type errors in consuming components

---

### MVP-7: Update Home Component

**File:** `user-interface/flow-web/src/pages/Home.tsx`

**Changes:**
```tsx
const getDisplayName = (space: Space): string => {
  if (space.name) {
    return space.name;
  }
  const segments = space.location.split('/').filter(Boolean);
  return segments[segments.length - 1] || space.key.substring(0, 8);
};

// In JSX:
<h3 title={getDisplayName(space)}>{getDisplayName(space)}</h3>
<p className="space-location" title={space.location}>
  {space.location}
</p>
```

**Acceptance Criteria:**
- [ ] Space cards show name (not hash)
- [ ] Title attributes present for tooltips

---

### MVP-8: Update CSS

**File:** `user-interface/flow-web/src/pages/Home.css`

**Changes:**
```css
.space-info h3 {
  margin: 0 0 0.5rem;
  font-size: 1.125rem;
  color: #fff;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

.space-location {
  margin: 0 0 0.25rem;
  font-size: 0.75rem;
  color: #888;
  font-family: monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
```

**Acceptance Criteria:**
- [ ] Long names truncate with ellipsis
- [ ] Long paths truncate with ellipsis
- [ ] No content overflows card boundaries

---

### MVP-9: Backend Unit Tests

**Scope:** Verify name derivation logic

**Test Cases:**
- Normal path → last segment
- Trailing slash → handled correctly
- Root path → hash fallback
- Unicode → preserved
- Duplicates → hash suffix added

**Acceptance Criteria:**
- [ ] All existing tests pass
- [ ] New name derivation tests pass

---

### MVP-10: Manual UI Verification

**Checklist:**
- [ ] Create new space → name derived correctly
- [ ] Existing spaces → names backfilled
- [ ] Long name → truncates with ellipsis
- [ ] Long path → truncates with ellipsis
- [ ] Hover → tooltip shows full text
- [ ] Different browsers → consistent rendering

---

### MVP-11: Backend Audit

**Scope:** Manual code review of `back-end/node/src/`

**Methodology:**
1. Run `tokei back-end/node/src/` for metrics
2. Review each module for:
   - Traits with single implementations
   - Wrapper types adding no behavior
   - Excessive configuration complexity
   - Over-granular error types
3. Score by Impact / Effort ratio
4. Categorize as High/Medium/Low priority

**Acceptance Criteria:**
- [ ] All modules reviewed
- [ ] Findings documented with format: Issue, Why, Recommendation, Effort

---

### MVP-12: Document Findings

**Output:** `specs/02_simplification_ui/audit_report.md`

**Template:**
```markdown
# Backend Audit Report

**Date:** 2026-01-10
**Reviewer:** [Name]
**Scope:** back-end/node/src/

## Summary

| Priority | Count | Total Effort |
|----------|-------|--------------|
| High     | X     | Xh           |
| Medium   | X     | Xh           |
| Low      | X     | Xh           |

## Findings

### Finding 1: [Title]
- **File:** path/to/file.rs
- **Priority:** High/Medium/Low
- **Issue:** [Description]
- **Recommendation:** [Action]
- **Effort:** Xh

[Repeat for each finding]
```

**Acceptance Criteria:**
- [ ] Report follows template
- [ ] All findings actionable
- [ ] Effort estimates provided

---

## 6. MVP Deployment

### 6.1 Pre-Deployment Checklist

- [ ] Backup database: `cp /Users/julian/db/data.sqlite /Users/julian/db/data.sqlite.backup.$(date +%Y%m%d)`
- [ ] All tests passing
- [ ] Manual UI verification complete

### 6.2 Deployment Steps

1. Run migration: `cargo run -p migration -- up`
2. Verify backfill: `sqlite3 /Users/julian/db/data.sqlite "SELECT id, name FROM space"`
3. Start backend: `cargo run -p node`
4. Start frontend: `cd user-interface/flow-web && npm run dev`
5. Verify in browser

### 6.3 Rollback

```bash
# If issues:
cargo run -p migration -- down
# Or restore backup:
cp /Users/julian/db/data.sqlite.backup.YYYYMMDD /Users/julian/db/data.sqlite
```

---

## 7. Post-MVP Roadmap

| Feature | Priority | When |
|---------|----------|------|
| Rename UI | P2 | After P2P Phase 1 |
| Name search/filter | P3 | Future |
| Bulk rename | P3 | Future |

---

## 8. Technical Debt Created

| Simplification | Debt Description | Remediation |
|----------------|------------------|-------------|
| None | This MVP is production-ready | N/A |

**Note:** This MVP creates zero technical debt. All code patterns match the full technical specification and require no rework.

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-01-10 | Engineering | Initial MVP spec |
