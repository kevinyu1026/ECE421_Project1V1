//! Database module to handle player registration, player stats, and login using SQLite

use sqlx::{SqlitePool, Row};
use uuid::Uuid;
use std::sync::Arc;

#[derive(Debug)]
pub struct PlayerStats {
    pub games_played: i32,
    pub games_won: i32,
}

#[derive(Debug)]
pub struct Player {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct Database {
    pub pool: Arc<SqlitePool>,
}

impl Database {
    pub fn new(pool: SqlitePool) -> Self {
        Database {
            pool: Arc::new(pool),
        }
    }

    pub async fn register_player(&self, name: &str) -> Result<String, sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO players (id, name) VALUES (?1, ?2)")
            .bind(&id)
            .bind(name)
            .execute(&*self.pool)
            .await?;
        Ok(id)
    }

    pub async fn login_player(&self, name: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query("SELECT id FROM players WHERE name = ?1")
            .bind(name)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(row.map(|r| r.get(0)))
    }

    pub async fn player_stats(&self, player_id: &str) -> Result<PlayerStats, sqlx::Error> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS games_played, SUM(won) AS games_won FROM games WHERE player_id = ?1",
        )
        .bind(player_id)
        .fetch_one(&*self.pool)
        .await?;

        Ok(PlayerStats {
            games_played: row.get(0),
            games_won: row.get(1),
        })
    }

    pub async fn list_players(&self) -> Result<Vec<Player>, sqlx::Error> {
        let rows = sqlx::query("SELECT id, name FROM players")
            .fetch_all(&*self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| Player {
                id: row.get(0),
                name: row.get(1),
            })
            .collect())
    }
}
