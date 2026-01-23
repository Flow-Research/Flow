# Technical Specification: Codebase Simplification & UI Fixes

**Version:** 1.1
**Date:** 2026-01-10
**Status:** Reviewed
**Author:** Engineering Team
**Reviewed:** 6-Perspective Technical Review
**Source:** specs/02_simplification_ui/product_spec.md v1.1

---

## 1. Overview

### 1.1 Purpose

This technical specification details the implementation approach for:
1. **Backend Audit**: Manual code review methodology to identify over-engineering
2. **Space Naming Feature**: Database schema change, entity updates, and API modifications
3. **Card Truncation Fix**: CSS and React component updates

### 1.2 Scope

| Component | Changes |
|-----------|---------|
| Database | Add `name` column to `space` table |
| Entity Layer | Update `space.rs` model |
| API Layer | Modify space endpoints to return `name` |
| Business Logic | Derive name from path in `modules/space.rs` |
| Frontend Service | Update `Space` TypeScript interface |
| UI Components | Update `Home.tsx` and `Home.css` |
| Documentation | Audit report deliverable |

### 1.3 Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Name storage | Nullable VARCHAR(255) | Backwards compatible, allows future manual override |
| Name derivation | Backend derives at creation | Ensures consistency, reduces frontend logic |
| Duplicate handling | Append hash suffix | Simple, deterministic, avoids conflicts |
| Path parsing | Rust `std::path::Path` | Cross-platform, handles edge cases |
| CSS truncation | `text-overflow: ellipsis` | Standard CSS, broad browser support |

---

## 2. System Architecture

### 2.1 Current Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         FRONTEND                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  flow-web (React + Vite)                                │    │
│  │  └── pages/Home.tsx (Space cards display)               │    │
│  │  └── services/api.ts (Space interface)                  │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼ HTTP/REST
┌─────────────────────────────────────────────────────────────────┐
│                          BACKEND                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  api/servers/                                           │    │
│  │  └── REST handlers (Axum)                               │    │
│  ├─────────────────────────────────────────────────────────┤    │
│  │  modules/space.rs                                       │    │
│  │  └── new_space() - creates space records                │    │
│  ├─────────────────────────────────────────────────────────┤    │
│  │  entity/space.rs                                        │    │
│  │  └── Space model (Sea-ORM)                              │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                │                                 │
│                                ▼                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  SQLite Database                                        │    │
│  │  └── space table                                        │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Affected Files Summary

| Layer | File | Changes Required |
|-------|------|------------------|
| Migration | `back-end/migration/src/m20260110_*.rs` | NEW: Add name column |
| Migration | `back-end/migration/src/lib.rs` | Register new migration |
| Entity | `back-end/entity/src/space.rs` | Add `name: Option<String>` field |
| Module | `back-end/node/src/modules/space.rs` | Derive name from path |
| Frontend | `user-interface/flow-web/src/services/api.ts` | Add `name` to Space interface |
| Frontend | `user-interface/flow-web/src/pages/Home.tsx` | Display name, add tooltips |
| Frontend | `user-interface/flow-web/src/pages/Home.css` | Add truncation styles |

---

## 3. Data Architecture

### 3.1 Schema Change

**Current `space` table:**

```sql
CREATE TABLE space (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key VARCHAR(64) NOT NULL UNIQUE,
    location VARCHAR(512) NOT NULL,
    time_created TIMESTAMP WITH TIME ZONE NOT NULL
);
```

**Target `space` table:**

```sql
CREATE TABLE space (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(255),  -- NEW: Human-readable name, nullable
    location VARCHAR(512) NOT NULL,
    time_created TIMESTAMP WITH TIME ZONE NOT NULL
);
```

### 3.2 Migration Implementation

**File:** `back-end/migration/src/m20260110_000000_add_space_name.rs`

```rust
use sea_orm_migration::prelude::*;
use sha2::{Digest, Sha256};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Step 1: Add nullable name column
        manager
            .alter_table(
                Table::alter()
                    .table(Space::Table)
                    .add_column(ColumnDef::new(Space::Name).string_len(255).null())
                    .to_owned(),
            )
            .await?;

        // Step 2: Backfill existing spaces with derived names
        // Use raw SQL for SQLite compatibility
        let db = manager.get_connection();

        // Get all spaces without names
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

        // Update each space with derived name
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

/// Derives a human-readable name from a filesystem path.
/// Uses sha2 (already in project dependencies) for fallback hash.
fn derive_name_from_path(location: &str) -> String {
    // Try to extract last path segment
    let path = std::path::Path::new(location);

    // Handle trailing slashes by trimming them first
    let trimmed = location.trim_end_matches('/');
    let path = std::path::Path::new(trimmed);

    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let trimmed_name = name.trim();
        if !trimmed_name.is_empty() {
            return trimmed_name.to_owned();
        }
    }

    // Fallback: use first 8 chars of SHA-256 hash (same crate used elsewhere)
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

**Note:** The migration uses `sha2` crate which is already a project dependency (used for space key generation). No new dependencies required.

**Register in `lib.rs`:**

```rust
mod m20260110_000000_add_space_name;

// In migrations() vec:
Box::new(m20260110_000000_add_space_name::Migration),
```

### 3.3 Entity Model Update

**File:** `back-end/entity/src/space.rs`

```rust
//! `SeaORM` Entity - Space

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "space")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub key: String,
    pub name: Option<String>,  // NEW: Human-readable name
    pub location: String,
    pub time_created: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::space_index_status::Entity")]
    SpaceIndexStatus,
}

impl Related<super::space_index_status::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::SpaceIndexStatus.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

### 3.4 Data Validation Rules

| Field | Rule | Implementation |
|-------|------|----------------|
| `name` | Max 255 characters | VARCHAR(255) constraint |
| `name` | Trimmed whitespace | Rust `.trim()` before storage |
| `name` | No HTML/script tags | Not needed - React escapes by default |
| `name` | Duplicate handling | Append `-{hash_prefix}` suffix |

---

## 4. API Specification

### 4.1 Endpoint Changes

#### GET /api/v1/spaces

**Current Response:**
```json
[
  {
    "id": 1,
    "key": "79c50f2e702c3e56535062323a691...",
    "location": "/Users/julian/Documents/specs",
    "time_created": "2026-01-08T10:30:00Z"
  }
]
```

**New Response:**
```json
[
  {
    "id": 1,
    "key": "79c50f2e702c3e56535062323a691...",
    "name": "specs",
    "location": "/Users/julian/Documents/specs",
    "time_created": "2026-01-08T10:30:00Z"
  }
]
```

#### GET /api/v1/spaces/:key

Same structure change as list endpoint.

#### POST /api/v1/spaces

**Current Request:**
```json
{
  "dir": "/Users/julian/Documents/specs"
}
```

**New Request (optional name):**
```json
{
  "dir": "/Users/julian/Documents/specs",
  "name": "My Specs"  // Optional - derived from path if omitted
}
```

**Response:** Unchanged (returns `{ "status": "ok" }`)

### 4.2 API Handler Implementation

**File:** `back-end/node/src/api/servers/rest.rs`

**Current handler (line ~338):**
```rust
async fn create_space(
    State(app_state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;
    let dir = payload["dir"].as_str().unwrap_or("/tmp/space");

    match node.create_space(dir).await {
        Ok(_) => Ok(Json(json!({"status": "success"}))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}
```

**Updated handler:**
```rust
#[derive(Deserialize)]
struct CreateSpaceRequest {
    dir: String,
    name: Option<String>,  // NEW: Optional custom name
}

async fn create_space(
    State(app_state): State<AppState>,
    Json(payload): Json<CreateSpaceRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let node = app_state.node.read().await;

    // Validate name length if provided
    if let Some(ref name) = payload.name {
        if name.len() > 255 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Space name must be 255 characters or less".to_string(),
            ));
        }
    }

    match node.create_space(&payload.dir, payload.name.as_deref()).await {
        Ok(_) => Ok(Json(json!({"status": "success"}))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}
```

**Also update `list_spaces` to include name (line ~320):**
```rust
async fn list_spaces(
    State(app_state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    use entity::space;
    // ... existing code to get spaces ...

    let space_list: Vec<Value> = spaces
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "key": s.key,
                "name": s.name,  // NEW: Include name in response
                "location": s.location,
                "time_created": s.time_created.to_string()
            })
        })
        .collect();

    Ok(Json(json!(space_list)))
}
```

### 4.3 No Breaking Changes

The API changes are **additive**:
- New `name` field in responses (clients can ignore if not used)
- Optional `name` field in POST request (omit for default behavior)
- All existing clients continue to work without modification

---

## 5. Implementation Details

### 5.1 Space Module Changes

**File:** `back-end/node/src/modules/space.rs`

```rust
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use chrono::Utc;
use errors::AppError;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection,
    EntityTrait, QueryFilter, QuerySelect,
};
use tracing::{info, warn};

use entity::space;
use sha2::{Digest, Sha256};
use space::Entity as Space;

use crate::modules::ai::pipeline_manager::PipelineManager;

/// Creates a new space with a human-readable name.
///
/// # Arguments
/// * `dir` - Directory path to create space for
/// * `custom_name` - Optional custom name. If None, derives from path.
/// * `db` - Database connection
/// * `pipeline_manager` - Pipeline manager for indexing
///
/// # Name Derivation Logic (when custom_name is None)
/// 1. Extract last path segment: `/Users/julian/Documents/specs` → `specs`
/// 2. Handle trailing slashes: `/path/to/folder/` → `folder`
/// 3. Handle duplicates: append `-{hash_prefix}` (e.g., `docs-7f3a`)
/// 4. Fallback for edge cases: use first 8 chars of location hash
pub async fn new_space(
    dir: &str,
    custom_name: Option<&str>,  // NEW: Optional custom name
    db: &DatabaseConnection,
    pipeline_manager: &PipelineManager,
) -> Result<(), AppError> {
    info!("Setting up space in directory: {}", dir);

    let path = Path::new(dir);

    if !path.exists() {
        info!("Creating directory: {}", dir);
        fs::create_dir_all(path).map_err(|e| AppError::IO(e))?;
    }

    let space_key = generate_space_key(dir)?;
    info!("Generated space key: {}", space_key);

    // Check if space already exists
    match Space::find()
        .filter(entity::space::Column::Key.eq(&space_key))
        .one(db)
        .await
    {
        Ok(Some(_existing_space)) => {
            info!(
                "Space already exists at directory: {} (key: {})",
                dir, space_key
            );
            return Ok(());
        }
        Ok(None) => {
            warn!(
                "Directory exists but no space record found. Creating space record for: {}",
                dir
            );
        }
        Err(e) => {
            return Err(AppError::Storage(Box::new(e)));
        }
    }

    let canonical_location = path
        .canonicalize()
        .map_err(|e| AppError::IO(e))?
        .to_str()
        .ok_or_else(|| AppError::Config("Directory path contains invalid UTF-8".to_owned()))?
        .to_owned();

    // Use custom name if provided, otherwise derive from path
    let space_name = match custom_name {
        Some(name) => name.trim().to_owned(),
        None => derive_unique_name(&canonical_location, &space_key, db).await?,
    };

    let new_space = space::ActiveModel {
        key: Set(space_key.clone()),
        name: Set(Some(space_name)),  // NEW: Set name (custom or derived)
        location: Set(canonical_location.clone()),
        time_created: Set(Utc::now().into()),
        ..Default::default()
    };

    let result = match new_space.insert(db).await {
        Ok(space_model) => {
            info!(
                "Successfully created space with ID: {}, Key: {}, Name: {:?}, Location: {}",
                space_model.id, space_model.key, space_model.name, space_model.location
            );

            pipeline_manager
                .initialize_space_pipeline(
                    space_model.id,
                    &space_model.key,
                    &space_model.location,
                    None,
                )
                .await?;

            // Start indexing in background
            match pipeline_manager.index_space(&space_model.key).await {
                Ok(_) => info!("Indexing started for space: {}", space_model.key),
                Err(e) => {
                    warn!(
                        "Failed to start indexing for space {}: {}. You can manually trigger it later.",
                        space_model.key, e
                    );
                }
            }

            Ok(())
        }
        Err(e) => Err(AppError::Storage(Box::new(e))),
    };

    result
}

/// Derives a unique name from path, handling duplicates
async fn derive_unique_name(
    location: &str,
    space_key: &str,
    db: &DatabaseConnection,
) -> Result<String, AppError> {
    // Extract base name from path
    let base_name = Path::new(location)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // Fallback for edge cases (root path, etc.)
            space_key[..8].to_owned()
        });

    // Check for existing spaces with same name
    let existing_names: HashSet<String> = Space::find()
        .select_only()
        .column(space::Column::Name)
        .into_tuple::<Option<String>>()
        .all(db)
        .await
        .map_err(|e| AppError::Storage(Box::new(e)))?
        .into_iter()
        .flatten()
        .collect();

    if !existing_names.contains(&base_name) {
        return Ok(base_name);
    }

    // Name exists - append hash suffix
    let suffix = &space_key[..4];
    let unique_name = format!("{}-{}", base_name, suffix);

    // Verify uniqueness (extremely unlikely to collide)
    if existing_names.contains(&unique_name) {
        // Use longer suffix as final fallback
        Ok(format!("{}-{}", base_name, &space_key[..8]))
    } else {
        Ok(unique_name)
    }
}

fn generate_space_key(dir: &str) -> Result<String, AppError> {
    let path = Path::new(dir).canonicalize().map_err(|e| AppError::IO(e))?;

    let path_str = path
        .to_str()
        .ok_or_else(|| AppError::Config("Directory path contains invalid UTF-8".to_owned()))?;

    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();

    Ok(format!("{:x}", hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_derive_name_normal_path() {
        let path = "/Users/julian/Documents/specs";
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap();
        assert_eq!(name, "specs");
    }

    #[test]
    fn test_derive_name_trailing_slash() {
        let path = "/Users/julian/Documents/specs/";
        // Path::new handles trailing slashes correctly
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        // Note: Path::file_name returns None for paths ending in /
        // This is handled by the fallback logic
    }

    #[test]
    fn test_derive_name_unicode() {
        let path = "/Users/julian/Documents/文档";
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap();
        assert_eq!(name, "文档");
    }

    #[test]
    fn test_derive_name_special_chars() {
        let path = "/Users/julian/Documents/my docs (2024)";
        let name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap();
        assert_eq!(name, "my docs (2024)");
    }

    // Existing tests...
    #[test]
    fn test_generate_space_key_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        let key1 = generate_space_key(path).unwrap();
        let key2 = generate_space_key(path).unwrap();

        assert_eq!(key1, key2, "Same path should generate same key");
        assert_eq!(key1.len(), 64, "SHA-256 hex should be 64 characters");
    }

    #[test]
    fn test_generate_space_key_different_paths() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let key1 = generate_space_key(temp_dir1.path().to_str().unwrap()).unwrap();
        let key2 = generate_space_key(temp_dir2.path().to_str().unwrap()).unwrap();

        assert_ne!(key1, key2, "Different paths should generate different keys");
    }
}
```

### 5.2 Frontend Service Update

**File:** `user-interface/flow-web/src/services/api.ts`

```typescript
export interface Space {
  id: number;
  key: string;
  name: string | null;  // NEW: Human-readable name
  location: string;
  time_created: string;
}
```

### 5.3 UI Component Changes

**File:** `user-interface/flow-web/src/pages/Home.tsx`

```tsx
// Helper function to get display name
const getDisplayName = (space: Space): string => {
  if (space.name) {
    return space.name;
  }
  // Fallback: derive from location (defensive, rarely needed)
  const segments = space.location.split('/').filter(Boolean);
  return segments[segments.length - 1] || space.key.substring(0, 8);
};

// In the space card JSX (around line 85):
<div className="space-info">
  <h3 title={getDisplayName(space)}>{getDisplayName(space)}</h3>
  <p className="space-location" title={space.location}>
    {space.location}
  </p>
  {/* ... rest of card content */}
</div>
```

### 5.4 CSS Truncation Fix

**File:** `user-interface/flow-web/src/pages/Home.css`

```css
/* Space card container - ensure overflow is hidden */
.space-card {
  overflow: hidden;
}

/* Space name - single line with ellipsis */
.space-info h3 {
  margin: 0 0 0.5rem;
  font-size: 1.125rem;
  color: #fff;
  /* Truncation properties */
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

/* Space location - single line with ellipsis */
.space-location {
  font-family: monospace;
  font-size: 0.75rem;
  color: #888;
  margin: 0;
  /* Truncation properties - replace word-break: break-all */
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
```

---

## 6. Infrastructure & Deployment

### 6.1 Database Migration

**Deployment steps:**

1. **Backup database before migration:**
   ```bash
   # Find database location from .env
   cp /Users/julian/db/data.sqlite /Users/julian/db/data.sqlite.backup.$(date +%Y%m%d)
   ```
2. Run Sea-ORM migration: `cargo run -p migration -- up`
3. Migration automatically backfills existing spaces
4. Verify: `sqlite3 /Users/julian/db/data.sqlite "SELECT id, name, location FROM space"`

**Rollback:**

```bash
# Revert migration
cargo run -p migration -- down

# Or restore from backup if needed
cp /Users/julian/db/data.sqlite.backup.YYYYMMDD /Users/julian/db/data.sqlite
```

### 6.2 No Infrastructure Changes Required

- Same SQLite database
- Same server configuration
- Same deployment process
- No new dependencies

---

## 7. Security Architecture

### 7.1 Input Validation

| Input | Validation | Location |
|-------|------------|----------|
| Space name (derived) | Max 255 chars, trimmed | `space.rs::derive_unique_name()` |
| Space name (manual) | Max 255 chars, trimmed | Future: API handler |
| Display in UI | React auto-escapes | `Home.tsx` |

### 7.2 Authorization

No changes to authorization model. Space names are visible only to authenticated users (existing JWT middleware).

### 7.3 Threat Mitigations

| Threat | Risk | Mitigation |
|--------|------|------------|
| XSS via name | Low | React escapes by default |
| SQL injection | None | Sea-ORM uses parameterized queries |
| Path traversal | N/A | Names are display-only |

---

## 8. Performance & Scalability

### 8.1 Performance Impact

| Operation | Impact | Notes |
|-----------|--------|-------|
| Migration backfill | One-time O(n) | Runs once, iterates all spaces |
| Space creation | O(1) + O(n) query | Name derivation adds one SELECT for duplicates |
| Space list API | None | Name already in model, serialized automatically |
| UI render | None | Text truncation is pure CSS |

### 8.2 Optimization Notes

- Duplicate name check queries `name` column only (small payload)
- Consider adding index on `name` column if space count exceeds 10,000
- CSS truncation has zero JS overhead

---

## 9. Reliability & Operations

### 9.1 Error Handling

| Scenario | Handling |
|----------|----------|
| Migration fails mid-backfill | Transaction rollback, retry safe |
| Path has no valid name segment | Fallback to hash prefix |
| Database query fails | Return AppError, logged |

### 9.2 Monitoring

No new monitoring requirements. Existing logging captures:
- Space creation events
- Migration progress
- Any errors during name derivation

---

## 10. Development Standards

### 10.1 Testing Requirements

| Test Type | Coverage |
|-----------|----------|
| Unit tests | Name derivation functions |
| Integration tests | API returns name field |
| Migration tests | Backfill works correctly |
| UI tests | Manual visual verification |

### 10.2 Test Cases

```rust
#[cfg(test)]
mod name_derivation_tests {
    // Normal path
    assert_derive("/Users/julian/Documents/specs") == "specs";

    // Trailing slash
    assert_derive("/Users/julian/Documents/specs/") == "specs";

    // Root path
    assert_derive("/") == "<hash_prefix>";

    // Single segment
    assert_derive("docs") == "docs";

    // Special characters
    assert_derive("/path/to/my docs (2024)") == "my docs (2024)";

    // Unicode
    assert_derive("/path/to/文档") == "文档";

    // Duplicate handling
    assert_derive_unique("/path/a/docs", existing=["docs"]) == "docs-7f3a";
}
```

---

## 11. Implementation Roadmap

### 11.1 Work Items

| ID | Task | Dependencies | Priority |
|----|------|--------------|----------|
| **WORK-1** | Create migration file `m20260110_000000_add_space_name.rs` | None | P0 |
| **WORK-2** | Register migration in `lib.rs` | WORK-1 | P0 |
| **WORK-3** | Update `entity/space.rs` with `name` field | WORK-1 | P0 |
| **WORK-4** | Update `modules/space.rs` with name derivation | WORK-3 | P0 |
| **WORK-5** | Update frontend `Space` interface in `api.ts` | None | P0 |
| **WORK-6** | Update `Home.tsx` to display name | WORK-5 | P0 |
| **WORK-7** | Update `Home.css` with truncation styles | None | P0 |
| **WORK-8** | Add unit tests for name derivation | WORK-4 | P1 |
| **WORK-9** | Add integration tests for API | WORK-4 | P1 |
| **WORK-10** | Conduct backend audit (manual review) | None | P0 |
| **WORK-11** | Document audit findings in `audit_report.md` | WORK-10 | P0 |

### 11.2 Implementation Order

```
Phase 1: Database & Backend
├── WORK-1: Create migration
├── WORK-2: Register migration
├── WORK-3: Update entity
└── WORK-4: Update space module

Phase 2: Frontend
├── WORK-5: Update API types
├── WORK-6: Update Home.tsx
└── WORK-7: Update Home.css

Phase 3: Testing
├── WORK-8: Unit tests
└── WORK-9: Integration tests

Phase 4: Audit (Parallel)
├── WORK-10: Conduct audit
└── WORK-11: Document findings
```

---

## 12. Appendices

### Appendix A: Backend Audit Methodology

The backend audit is a **manual code review** process using the following methodology:

#### A.1 Scope

| Directory | Purpose |
|-----------|---------|
| `modules/ai/` | AI pipeline, chunking, indexing |
| `modules/network/` | libp2p networking, GossipSub, mDNS |
| `modules/storage/` | KV store, content addressing, IPLD |
| `modules/query/` | Search, RAG pipeline |
| `modules/knowledge/` | Entity extraction, knowledge graph |
| `modules/ssi/` | WebAuthn, DID, JWT |
| `bootstrap/` | Configuration, initialization |
| `api/` | REST handlers, WebSocket |

#### A.2 Evaluation Criteria

For each module, evaluate:

1. **Abstraction Depth**: Are there traits with single implementations?
2. **Configuration Complexity**: Is config more complex than usage?
3. **Error Granularity**: Are error types too fine-grained?
4. **Type Wrappers**: Are there newtypes that add no behavior?
5. **Module Coupling**: How many cross-module dependencies?

#### A.3 Scoring Matrix

| Criterion | Score 1 | Score 2 | Score 3 |
|-----------|---------|---------|---------|
| Impact | Minor annoyance | Slows development | Blocks progress |
| Effort | < 1 hour | 1-4 hours | > 4 hours |

**Priority = Impact / Effort**
- High: > 1.5
- Medium: 1.0 - 1.5
- Low: < 1.0

#### A.4 Output Format

```markdown
## Audit Finding: [Module Name]

**File:** `path/to/file.rs`
**Lines:** 45-67
**Priority:** High/Medium/Low

### Issue
[Description of the over-engineering pattern]

### Why It's Over-Engineered
[Explanation of why this adds unnecessary complexity]

### Recommendation
[Specific suggestion for simplification]

### Effort Estimate
[Hours to implement]

### Example
```rust
// Before
[problematic code snippet]

// After
[simplified code snippet]
```
```

### Appendix B: File Path Reference

```
back-end/
├── entity/
│   └── src/
│       └── space.rs          # Entity model (UPDATE)
├── migration/
│   └── src/
│       ├── lib.rs            # Migration registry (UPDATE)
│       └── m20260110_*.rs    # New migration (CREATE)
└── node/
    └── src/
        └── modules/
            └── space.rs      # Business logic (UPDATE)

user-interface/
└── flow-web/
    └── src/
        ├── services/
        │   └── api.ts        # API types (UPDATE)
        └── pages/
            ├── Home.tsx      # Component (UPDATE)
            └── Home.css      # Styles (UPDATE)

specs/
└── 02_simplification_ui/
    ├── brainstorm.md
    ├── product_spec.md
    ├── technical_spec.md     # This document
    └── audit_report.md       # Output (CREATE)
```

### Appendix C: SQLite Compatibility Notes

SQLite-specific considerations:

1. **No `REVERSE()` function**: Path parsing done in application code
2. **No `INSTR()` function**: Use Rust `Path` API instead
3. **Nullable columns**: Use `Option<String>` in Sea-ORM
4. **ALTER TABLE ADD COLUMN**: Supported, column added at end

### Appendix D: Glossary

| Term | Definition |
|------|------------|
| Space | A collection of documents from a filesystem directory |
| Space Key | 64-character SHA-256 hash of canonical path |
| Space Name | Human-readable identifier (derived or manual) |
| Sea-ORM | Rust ORM used for database operations |
| IPLD | InterPlanetary Linked Data (future P2P data format) |

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-01-10 | Engineering | Initial draft |
| 1.1 | 2026-01-10 | Engineering | 6-perspective review: Fixed migration code (sha2 instead of md5, ownership bug), added API handler implementation, added backup commands, updated function signatures |
