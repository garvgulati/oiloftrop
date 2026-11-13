//! Database admission before migration/service initialization. The marker lives
//! inside the profile database and inherits its encryption and backup protection.
use rusqlite::OptionalExtension;
use wealthfolio_core::{
    errors::{DatabaseError, Error, Result, ValidationError},
    runtime_boundary::{RuntimePurpose, RUNTIME_BOUNDARY_VERSION},
};

use super::DbAccess;

#[derive(Debug)]
pub struct BoundaryAdmission {
    purpose: RuntimePurpose,
    path: String,
}

impl DbAccess {
    /// Call while holding DatabaseOwner, before migrations can change the file.
    pub fn check_runtime_boundary(&self, purpose: RuntimePurpose) -> Result<BoundaryAdmission> {
        if purpose == RuntimePurpose::Personal && !self.is_encrypted() {
            return Err(rejected("Personal profiles require database encryption."));
        }
        let conn = self.connect_rusqlite()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='runtime_boundary')",
            [], |row| row.get(0),
        ).map_err(query_failed)?;
        if exists {
            let marker = conn
                .query_row(
                    "SELECT purpose, boundary_version FROM runtime_boundary WHERE singleton=1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
                )
                .optional()
                .map_err(query_failed)?;
            if let Some((stored, version)) = marker {
                if stored != purpose.as_str() || version != RUNTIME_BOUNDARY_VERSION {
                    return Err(rejected("The profile purpose or boundary version does not match this runtime. Restore a compatible installation."));
                }
                return Ok(BoundaryAdmission {
                    purpose,
                    path: self.path().to_string(),
                });
            }
        }
        if purpose == RuntimePurpose::Personal {
            let populated: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*' AND name NOT IN ('runtime_boundary', '__diesel_schema_migrations'))",
                [], |row| row.get(0),
            ).map_err(query_failed)?;
            if populated {
                return Err(rejected("An existing unclassified profile cannot be adopted as personal. Create a fresh encrypted personal profile."));
            }
        }
        Ok(BoundaryAdmission {
            purpose,
            path: self.path().to_string(),
        })
    }

    /// Seal once after successful migrations, before any service/writer starts.
    /// Do not use this method to relabel a profile or change its purpose.
    pub fn seal_runtime_boundary(&self, admission: BoundaryAdmission) -> Result<()> {
        if admission.path != self.path() {
            return Err(rejected(
                "The boundary admission belongs to a different profile.",
            ));
        }
        let mut conn = self.connect_rusqlite()?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(query_failed)?;
        tx.execute(
            "INSERT OR IGNORE INTO runtime_boundary(singleton,purpose,boundary_version) VALUES(1,?1,?2)",
            rusqlite::params![admission.purpose.as_str(), RUNTIME_BOUNDARY_VERSION],
        ).map_err(query_failed)?;
        let (purpose, version): (String, u32) = tx
            .query_row(
                "SELECT purpose,boundary_version FROM runtime_boundary WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(query_failed)?;
        if purpose != admission.purpose.as_str() || version != RUNTIME_BOUNDARY_VERSION {
            return Err(rejected("The profile boundary changed during admission."));
        }
        tx.commit().map_err(query_failed)?;
        Ok(())
    }
}

fn query_failed(error: rusqlite::Error) -> Error {
    // Database errors here contain no imported financial fields or credentials.
    DatabaseError::QueryFailed(error.to_string()).into()
}

fn rejected(message: &str) -> Error {
    ValidationError::InvalidInput(message.to_string()).into()
}
